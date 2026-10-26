//! A port-forward from this computer to a Service that only the cluster reaches, such as the
//! registry `zot.zot.svc:5000`. It goes through kube-apiserver with the person's own rights
//! (`pods/portforward`), as `kubectl port-forward` does.

use std::net::Ipv4Addr;

use kube::Api;
use kube::api::{ListParams, PostParams};
use zeabur_kube::k8s_openapi::api::authorization::v1::{
    ResourceAttributes, SelfSubjectAccessReview, SelfSubjectAccessReviewSpec,
};
use zeabur_kube::k8s_openapi::api::core::v1::{Pod, Service};
use zeabur_kube::k8s_openapi::api::discovery::v1::EndpointSlice;
use zeabur_kube::k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;

use crate::output;

/// A Service port, from its in-cluster address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ServiceAddress {
    pub(crate) namespace: String,
    pub(crate) name: String,
    pub(crate) port: u16,
}

impl ServiceAddress {
    /// `<name>.<namespace>.svc[.cluster.local][:port]` (port 443 when left out); `None` for
    /// another address.
    pub(crate) fn parse(address: &str) -> Option<Self> {
        let address = address.strip_prefix("https://").unwrap_or(address);
        let (host, port) = match address.rsplit_once(':') {
            Some((host, port)) => (host, port.parse().ok()?),
            None => (address, 443),
        };
        let host = host.strip_suffix(".cluster.local").unwrap_or(host);
        let mut labels = host.strip_suffix(".svc")?.split('.');
        let (Some(name), Some(namespace), None) = (labels.next(), labels.next(), labels.next())
        else {
            return None;
        };
        (!name.is_empty() && !namespace.is_empty()).then(|| Self {
            namespace: namespace.to_owned(),
            name: name.to_owned(),
            port,
        })
    }
}

impl std::fmt::Display for ServiceAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}:{}", self.namespace, self.name, self.port)
    }
}

/// Listens on a loopback port and forwards each connection to a ready pod of `service`. Gives
/// the port.
pub(crate) async fn start(client: kube::Client, service: &ServiceAddress) -> Result<u16, String> {
    let (pod, port) = target(&client, service).await.map_err(|e| {
        format!(
            "Cannot port-forward to the registry {service}: {e}. Local mode needs `get services`, `list endpointslices` and `create pods/portforward` in {}. An administrator has them.",
            service.namespace
        )
    })?;
    may_forward(&client, &pod, &service.namespace).await?;
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .map_err(|e| format!("cannot listen on 127.0.0.1: {e}"))?;
    let local = listener.local_addr().map_err(|e| e.to_string())?.port();
    let pods: Api<Pod> = Api::namespaced(client, &service.namespace);
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let (pods, pod) = (pods.clone(), pod.clone());
            tokio::spawn(async move {
                let mut forwarder = match pods.portforward(&pod, &[port]).await {
                    Ok(forwarder) => forwarder,
                    Err(error) => {
                        output::error(&format!("The port-forward to {pod} failed: {error}"));
                        return;
                    }
                };
                if let Some(mut stream) = forwarder.take_stream(port) {
                    drop(tokio::io::copy_bidirectional(&mut socket, &mut stream).await);
                }
                drop(forwarder.join().await);
            });
        }
    });
    Ok(local)
}

/// Asks kube-apiserver whether the kubeconfig may forward a port to `pod`, so a missing right is
/// told at the start and not as a refused connection halfway through the upload.
async fn may_forward(client: &kube::Client, pod: &str, namespace: &str) -> Result<(), String> {
    let review = SelfSubjectAccessReview {
        spec: SelfSubjectAccessReviewSpec {
            resource_attributes: Some(ResourceAttributes {
                namespace: Some(namespace.to_owned()),
                verb: Some("create".to_owned()),
                resource: Some("pods".to_owned()),
                subresource: Some("portforward".to_owned()),
                name: Some(pod.to_owned()),
                ..ResourceAttributes::default()
            }),
            ..SelfSubjectAccessReviewSpec::default()
        },
        ..SelfSubjectAccessReview::default()
    };
    let answer = Api::<SelfSubjectAccessReview>::all(client.clone())
        .create(&PostParams::default(), &review)
        .await;
    // When the question cannot be asked, the upload tries and tells what it gets.
    if answer
        .ok()
        .and_then(|a| a.status)
        .is_some_and(|s| !s.allowed)
    {
        return Err(format!(
            "Your kubeconfig is not allowed to forward a port to the registry in the namespace {namespace}, which uploading a folder from this computer needs. Ask a cluster administrator for `create` on `pods/portforward` there, or upload from Zeabur's web interface."
        ));
    }
    Ok(())
}

/// A ready pod behind `service`, and the pod's port for it.
async fn target(client: &kube::Client, service: &ServiceAddress) -> Result<(String, u16), String> {
    let found = Api::<Service>::namespaced(client.clone(), &service.namespace)
        .get(&service.name)
        .await
        .map_err(|e| e.to_string())?;
    let entry = found
        .spec
        .and_then(|s| s.ports)
        .unwrap_or_default()
        .into_iter()
        .find(|p| i32::from(service.port) == p.port)
        .ok_or_else(|| format!("the Service has no port {}", service.port))?;
    let slices = Api::<EndpointSlice>::namespaced(client.clone(), &service.namespace)
        .list(
            &ListParams::default().labels(&format!("kubernetes.io/service-name={}", service.name)),
        )
        .await
        .map_err(|e| e.to_string())?;
    for slice in slices {
        let port = slice
            .ports
            .unwrap_or_default()
            .into_iter()
            .find(|p| p.name == entry.name)
            .and_then(|p| p.port)
            .or(match entry.target_port {
                Some(IntOrString::Int(port)) => Some(port),
                _ => None,
            });
        let pod = slice
            .endpoints
            .into_iter()
            .filter(|e| e.conditions.as_ref().and_then(|c| c.ready) != Some(false))
            .filter_map(|e| e.target_ref)
            .find(|r| r.kind.as_deref() == Some("Pod"))
            .and_then(|r| r.name);
        if let (Some(pod), Some(port)) = (pod, port.and_then(|p| u16::try_from(p).ok())) {
            return Ok((pod, port));
        }
    }
    Err("no pod of the Service is ready".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_in_cluster_service_address_is_forwarded() {
        let zot = ServiceAddress {
            namespace: "zot".to_owned(),
            name: "zot".to_owned(),
            port: 5000,
        };
        assert_eq!(ServiceAddress::parse("zot.zot.svc:5000"), Some(zot.clone()));
        assert_eq!(
            ServiceAddress::parse("https://zot.zot.svc.cluster.local:5000"),
            Some(zot)
        );
        assert_eq!(
            ServiceAddress::parse("registry.reg.svc").map(|s| s.port),
            Some(443)
        );
        for other in [
            "registry.example.com:5000",
            "ghcr.io",
            "a.b.c.svc:5000",
            "zot.svc:5000",
            "127.0.0.1:5000",
        ] {
            assert_eq!(ServiceAddress::parse(other), None, "{other}");
        }
    }
}
