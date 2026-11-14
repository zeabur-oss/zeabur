//! A [`Cluster`] that notes every call it passes on, for the cost runs and for tests that must
//! show a meaningful call was made or left out.
//!
//! The log names semantic calls (`namespace_objects shop`), not HTTP requests. One call can send
//! several requests and another none; the cost run counts the requests where the server sees them.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use k8s_openapi::api::core::v1::Namespace;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::api::DynamicObject;
use zeabur_core::problem::Denied;

use crate::objects::{AccessRules, ResourceRef, ServedResource};
use crate::port::{
    Answer, Cluster, ClusterEvents, ClusterFacts, ClusterPods, ExecRequest, ExecSession,
    ExecStream, ExitStatus, Identity, IssuedToken, LogLines, LogOptions, NamespaceObjects, RawList,
    ServicePort, WriteRequest, Written,
};

/// Wraps a [`Cluster`] and logs each call made through it.
#[derive(Debug)]
pub struct Recording<C> {
    inner: C,
    calls: Arc<Mutex<Vec<String>>>,
}

impl<C> Recording<C> {
    /// Wraps `inner` with an empty log.
    pub fn new(inner: C) -> Self {
        Self {
            inner,
            calls: Arc::default(),
        }
    }

    /// The calls so far, in the order they started.
    #[must_use]
    pub fn calls(&self) -> Vec<String> {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// How many times each distinct call was made.
    #[must_use]
    pub fn tally(&self) -> BTreeMap<String, usize> {
        let mut tally = BTreeMap::new();
        for call in self.calls() {
            *tally.entry(call).or_default() += 1;
        }
        tally
    }

    fn note(&self, call: String) {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(call);
    }
}

fn scoped(name: &str, resource: &ResourceRef, namespace: Option<&str>) -> String {
    format!("{name} {} {}", resource.key(), namespace.unwrap_or("*"))
}

impl<C: Cluster> Cluster for Recording<C> {
    fn whoami(&self) -> Answer<'_, Identity> {
        self.note("whoami".to_owned());
        self.inner.whoami()
    }

    fn namespaces(&self) -> Answer<'_, Vec<Namespace>> {
        self.note("namespaces".to_owned());
        self.inner.namespaces()
    }

    fn namespace_objects<'a>(&'a self, namespace: &'a str) -> Answer<'a, NamespaceObjects> {
        self.note(format!("namespace_objects {namespace}"));
        self.inner.namespace_objects(namespace)
    }

    fn namespace_workloads<'a>(&'a self, namespace: &'a str) -> Answer<'a, NamespaceObjects> {
        self.note(format!("namespace_workloads {namespace}"));
        self.inner.namespace_workloads(namespace)
    }

    fn version(&self) -> Answer<'_, String> {
        self.note("version".to_owned());
        self.inner.version()
    }

    fn readyz(&self) -> Answer<'_, String> {
        self.note("readyz".to_owned());
        self.inner.readyz()
    }

    fn kubeconfig_name(&self) -> Option<String> {
        self.inner.kubeconfig_name()
    }

    fn pod_logs<'a>(
        &'a self,
        namespace: &'a str,
        pod: &'a str,
        options: &'a LogOptions,
    ) -> Answer<'a, String> {
        self.note(format!("pod_logs {namespace}/{pod}"));
        self.inner.pod_logs(namespace, pod, options)
    }

    fn follow_pod_logs<'a>(
        &'a self,
        namespace: &'a str,
        pod: &'a str,
        options: &'a LogOptions,
    ) -> Answer<'a, LogLines> {
        self.note(format!("follow_pod_logs {namespace}/{pod}"));
        self.inner.follow_pod_logs(namespace, pod, options)
    }

    fn exec<'a>(
        &'a self,
        namespace: &'a str,
        pod: &'a str,
        request: &'a ExecRequest,
    ) -> Answer<'a, ExecSession> {
        self.note(format!("exec {namespace}/{pod}"));
        self.inner.exec(namespace, pod, request)
    }

    fn run<'a>(
        &'a self,
        namespace: &'a str,
        pod: &'a str,
        container: &'a str,
        command: &'a [String],
        input: &'a [u8],
    ) -> Answer<'a, ExitStatus> {
        self.note(format!("run {namespace}/{pod}"));
        self.inner.run(namespace, pod, container, command, input)
    }

    fn stream<'a>(
        &'a self,
        namespace: &'a str,
        pod: &'a str,
        container: &'a str,
        command: &'a [String],
    ) -> Answer<'a, ExecStream> {
        self.note(format!("stream {namespace}/{pod}"));
        self.inner.stream(namespace, pod, container, command)
    }

    fn reveal_secret<'a>(
        &'a self,
        namespace: &'a str,
        name: &'a str,
    ) -> Answer<'a, BTreeMap<String, Vec<u8>>> {
        self.note(format!("reveal_secret {namespace}/{name}"));
        self.inner.reveal_secret(namespace, name)
    }

    fn request_token<'a>(
        &'a self,
        namespace: &'a str,
        name: &'a str,
        seconds: i64,
    ) -> Answer<'a, IssuedToken> {
        self.note(format!("request_token {namespace}/{name}"));
        self.inner.request_token(namespace, name, seconds)
    }

    fn cluster_facts(&self) -> Answer<'_, ClusterFacts> {
        self.note("cluster_facts".to_owned());
        self.inner.cluster_facts()
    }

    fn cluster_pods(&self, limit: usize) -> Answer<'_, ClusterPods> {
        self.note("cluster_pods".to_owned());
        self.inner.cluster_pods(limit)
    }

    fn cluster_events(&self, limit: usize) -> Answer<'_, ClusterEvents> {
        self.note("cluster_events".to_owned());
        self.inner.cluster_events(limit)
    }

    fn serves<'a>(&'a self, group_version: &'a str) -> Answer<'a, Option<bool>> {
        self.note(format!("serves {group_version}"));
        self.inner.serves(group_version)
    }

    fn served_resources(&self) -> Answer<'_, Vec<ServedResource>> {
        self.note("served_resources".to_owned());
        self.inner.served_resources()
    }

    fn list_objects<'a>(
        &'a self,
        resource: &'a ResourceRef,
        namespace: Option<&'a str>,
    ) -> Answer<'a, Option<Vec<DynamicObject>>> {
        self.note(scoped("list_objects", resource, namespace));
        self.inner.list_objects(resource, namespace)
    }

    fn list_metadata<'a>(
        &'a self,
        resource: &'a ResourceRef,
        namespace: Option<&'a str>,
    ) -> Answer<'a, Option<Vec<ObjectMeta>>> {
        self.note(scoped("list_metadata", resource, namespace));
        self.inner.list_metadata(resource, namespace)
    }

    fn list_served_metadata<'a>(
        &'a self,
        resource: &'a ResourceRef,
        namespace: &'a str,
    ) -> Answer<'a, Option<Vec<ObjectMeta>>> {
        self.note(scoped("list_served_metadata", resource, Some(namespace)));
        self.inner.list_served_metadata(resource, namespace)
    }

    fn access_rules<'a>(&'a self, namespace: &'a str) -> Answer<'a, AccessRules> {
        self.note(format!("access_rules {namespace}"));
        self.inner.access_rules(namespace)
    }

    fn review_access<'a>(&'a self, request: &'a Denied) -> Answer<'a, bool> {
        self.note("review_access".to_owned());
        self.inner.review_access(request)
    }

    fn list_raw<'a>(
        &'a self,
        resource: &'a ResourceRef,
        namespace: Option<&'a str>,
    ) -> Answer<'a, RawList> {
        self.note(scoped("list", resource, namespace));
        self.inner.list_raw(resource, namespace)
    }

    fn list_labelled<'a>(
        &'a self,
        resource: &'a ResourceRef,
        namespace: Option<&'a str>,
        key: &'a str,
        value: &'a str,
    ) -> Answer<'a, Vec<ObjectMeta>> {
        self.note(scoped("list_labelled", resource, namespace));
        self.inner.list_labelled(resource, namespace, key, value)
    }

    fn get_object<'a>(
        &'a self,
        resource: &'a ResourceRef,
        namespace: Option<&'a str>,
        name: &'a str,
    ) -> Answer<'a, Option<DynamicObject>> {
        self.note(format!(
            "{} {name}",
            scoped("get_object", resource, namespace)
        ));
        self.inner.get_object(resource, namespace, name)
    }

    fn write<'a>(&'a self, request: &'a WriteRequest) -> Answer<'a, Written> {
        self.note(format!("write {}", request.target.name));
        self.inner.write(request)
    }

    fn proxy_get<'a>(
        &'a self,
        service: &'a ServicePort,
        path_and_query: &'a str,
    ) -> Answer<'a, serde_json::Value> {
        self.note(format!("proxy_get {service}"));
        self.inner.proxy_get(service, path_and_query)
    }
}
