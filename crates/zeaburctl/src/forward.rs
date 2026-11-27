//! A port-forward from this computer to a Service that only the cluster reaches, such as the
//! registry `zot.zot.svc:5000` or a database. It goes through kube-apiserver with the person's
//! own rights (`pods/portforward`), as `kubectl port-forward` does.

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

pub(crate) use zeabur_api::settings::ServiceAddress;

/// Listens on loopback port `local` (any free port for 0) and forwards each connection to a
/// ready pod of `service`. Gives the port.
pub(crate) async fn start(
    client: kube::Client,
    service: &ServiceAddress,
    local: u16,
) -> Result<u16, String> {
    let (pod, port) = target(&client, service).await.map_err(|e| {
        format!(
            "Cannot forward a port to {service}: {e}. This needs `get services`, `list endpointslices` and `create pods/portforward` in {}.",
            service.namespace
        )
    })?;
    may_forward(&client, &pod, service).await?;
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, local))
        .await
        .map_err(|e| format!("Cannot listen on 127.0.0.1:{local}: {e}."))?;
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
/// told at the start and not as a refused connection later.
async fn may_forward(
    client: &kube::Client,
    pod: &str,
    service: &ServiceAddress,
) -> Result<(), String> {
    let namespace = &service.namespace;
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
    // When the question cannot be asked, the forward tries and tells what it gets.
    if answer
        .ok()
        .and_then(|a| a.status)
        .is_some_and(|s| !s.allowed)
    {
        return Err(format!(
            "Your kubeconfig is not allowed to forward a port to {service}: it needs `create` on `pods/portforward` in the namespace {namespace}."
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
