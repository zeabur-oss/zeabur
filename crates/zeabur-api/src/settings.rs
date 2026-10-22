//! Zeabur's own settings: the metrics store and the cluster's name.
//!
//! # Where a setting is kept
//!
//! A setting has a default the server is started with (`ZEABUR_METRICS_SERVICE`), and a
//! person's choice, kept in the ConfigMap `zeabur-settings` in the namespace Zeabur runs in. The
//! ConfigMap is read and written as the person, like everything else, so RBAC decides who may
//! change a setting: whoever may update that ConfigMap (and create it, the first time), in
//! practice the administrators of Zeabur's namespace. Reading it needs `get` on it, which
//! Zeabur's chart grants to everyone signed in (`metricsStore.everyoneReads`; Zeabur itself still
//! has no rights of its own). Where it cannot be read, the default applies and the answer names
//! the refusal, so the interface can say which store it asked and why.
//!
//! Zeabur on a person's own computer (`zeaburctl ui`) runs in no namespace and keeps no
//! settings: the default is all there is, and a change is refused with the way to set it.
//!
//! # The metrics store
//!
//! A Service port that serves the Prometheus HTTP API, as `namespace/name:port`, that usage
//! history, node disk and volume use (`crate::cluster::disks`) are read from, through
//! kube-apiserver's service proxy (see `zeabur_kube::Cluster::proxy_get`). The default is where
//! Zeabur's recommendation installs VictoriaMetrics: the operator names a `VMSingle`'s Service
//! `vmsingle-<name>`, and the k8s-stack chart names its `VMSingle` after the release
//! (`vm-victoria-metrics-k8s-stack`), on port 8428, as the chart's own rendering shows
//! (`fixtures/charts/metrics__victoria-metrics.yaml`).
//!
//! `GET` says which Service is in use, where it came from, whether it answers and whether the
//! person may change it; `PUT` changes it (`null`: back to the default).
//!
//! # The cluster's name
//!
//! What the interface calls the cluster (see [`cluster_name`]). The chart's `clusterName`
//! (`ZEABUR_CLUSTER_NAME`) is the default; a name chosen in the settings replaces it.

pub mod cluster_name;

use zeabur_kube::ServicePort;

/// The ConfigMap people's settings are kept in, in Zeabur's namespace.
pub const CONFIG_MAP: &str = "zeabur-settings";

/// The ConfigMap's key for the metrics store.
pub const METRICS_STORE_KEY: &str = "metrics-store";

/// The ConfigMap's key for the cluster's name.
pub const CLUSTER_NAME_KEY: &str = "cluster-name";

/// Zeabur's settings as the server was started: the defaults, and where people's choices are
/// kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// The metrics store when no one has chosen another.
    pub metrics_store: ServicePort,
    /// The namespace whose `zeabur-settings` ConfigMap keeps people's choices; `None` when
    /// Zeabur runs on a person's computer, where they cannot be changed.
    pub namespace: Option<String>,
    /// The debug container's and the tools pod's images and limits.
    pub terminal: TerminalSettings,
    /// The cluster's name the server was started with (`ZEABUR_CLUSTER_NAME`).
    pub cluster_name: Option<String>,
    /// The host of the URL people open Zeabur at, when the server was started with one.
    pub public_host: Option<String>,
    /// Where Zeabur Identity keeps its people (see [`crate::members`]).
    pub identity: IdentitySettings,
}

/// Where Zeabur Identity keeps its people, and the address its setup links open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentitySettings {
    /// The namespace of its `User`, `Credential`, `Invitation` and `Grant` objects.
    pub namespace: String,
    /// Its issuer URL (`https://identity.example.com`), which setup links start with; `None`
    /// where the server was not started with one, and the Identity Deployment's is read instead.
    pub issuer: Option<String>,
}

impl Default for IdentitySettings {
    fn default() -> Self {
        Self {
            namespace: crate::members::DEFAULT_NAMESPACE.to_owned(),
            issuer: None,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            metrics_store: default_metrics_store(),
            namespace: None,
            terminal: TerminalSettings::default(),
            cluster_name: None,
            public_host: None,
            identity: IdentitySettings::default(),
        }
    }
}

/// The busybox 1.37.0 image index (`linux/amd64`, `linux/arm64` and others), by digest.
pub const DEBUG_IMAGE: &str = "docker.io/library/busybox:1.37.0@sha256:bdf57e528e45e4433820e045b29b4597825a1c9e38353532d90a01445013f82e";

/// The `alpine/k8s` 1.36.5 image index (`linux/amd64`, `linux/arm64`), by digest: `kubectl`
/// 1.36.5, `helm` and a busybox shell on Alpine.
pub const TOOLS_IMAGE: &str = "docker.io/alpine/k8s:1.36.5@sha256:094adf64cdc4827377cdbe7ea5747472ce115f9fbc64a4e35970ec66777f9a00";

/// What the debug container and the tools pod are made from (see `zeabur_plan::terminal` and the
/// `tools` module), from the chart's `terminal` values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalSettings {
    /// The debug container's image; it needs `sh`.
    pub debug_image: String,
    /// The tools pod's image; it needs `sh`, `sleep`, `kubectl` and `helm`.
    pub tools_image: String,
    /// The namespace tools pods go in; `None`: the project's.
    pub tools_namespace: Option<String>,
    /// How long a tools pod may run at most, in seconds (`activeDeadlineSeconds`).
    pub tools_deadline_seconds: i64,
    /// How long after its last terminal closes a tools pod is deleted, for a reconnect.
    pub tools_grace: std::time::Duration,
    /// The tools container's requests and limits.
    pub tools_resources: zeabur_plan::terminal::ToolsResources,
}

impl Default for TerminalSettings {
    fn default() -> Self {
        Self {
            debug_image: DEBUG_IMAGE.to_owned(),
            tools_image: TOOLS_IMAGE.to_owned(),
            tools_namespace: None,
            tools_deadline_seconds: 30 * 60,
            tools_grace: std::time::Duration::from_mins(1),
            tools_resources: zeabur_plan::terminal::ToolsResources::default(),
        }
    }
}

/// Where Zeabur's recommendation installs VictoriaMetrics (see the module documentation).
#[must_use]
pub fn default_metrics_store() -> ServicePort {
    ServicePort {
        namespace: "observability".to_owned(),
        name: "vmsingle-vm-victoria-metrics-k8s-stack".to_owned(),
        port: "8428".to_owned(),
    }
}

/// A Service port from `namespace/name:port`.
///
/// The namespace and the name are DNS labels, the port a number (1 to 65535) or a port name (an
/// IANA service name: at most 15 lower-case letters, digits and `-`, with a letter, not starting
/// or ending with `-`) [k8s-service-port].
///
/// # Errors
///
/// What is wrong with `text`, to show the person.
///
/// [k8s-service-port]: https://kubernetes.io/docs/reference/kubernetes-api/service-resources/service-v1/#ServiceSpec
pub fn parse_service_port(text: &str) -> Result<ServicePort, String> {
    let shape = || format!("{text:?} is not a Service port: write it as namespace/name:port.");
    let (namespace, rest) = text.trim().split_once('/').ok_or_else(shape)?;
    let (name, port) = rest.split_once(':').ok_or_else(shape)?;
    for (what, value) in [("namespace", namespace), ("Service name", name)] {
        if !zeabur_plan::checks::is_dns_label(value) {
            return Err(format!(
                "The {what} {value:?} is not a Kubernetes name (lower-case letters, digits and -)."
            ));
        }
    }
    let number = port.parse::<u16>().is_ok_and(|n| n > 0);
    let named = !port.is_empty()
        && port.len() <= 15
        && port
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && port.bytes().any(|b| b.is_ascii_lowercase())
        && !port.starts_with('-')
        && !port.ends_with('-')
        && !port.contains("--");
    if !(number || named) {
        return Err(format!(
            "The port {port:?} is neither a number from 1 to 65535 nor a port name."
        ));
    }
    Ok(ServicePort {
        namespace: namespace.to_owned(),
        name: name.to_owned(),
        port: port.to_owned(),
    })
}

/// The most characters a cluster's name may have.
pub const CLUSTER_NAME_MAX: usize = 63;

/// A cluster's name as a person or an installer wrote it: trimmed, at most
/// [`CLUSTER_NAME_MAX`] characters, no control characters.
///
/// # Errors
///
/// What is wrong with `text`, to show the person.
pub fn parse_cluster_name(text: &str) -> Result<String, String> {
    let name = text.trim();
    if name.is_empty() {
        return Err("A cluster's name must not be empty.".to_owned());
    }
    if name.chars().count() > CLUSTER_NAME_MAX {
        return Err(format!(
            "Choose a name with at most {CLUSTER_NAME_MAX} characters."
        ));
    }
    if name.chars().any(char::is_control) {
        return Err("A cluster's name must be on one line, without control characters.".to_owned());
    }
    Ok(name.to_owned())
}

pub use store::{MetricsStoreChange, MetricsStoreSetting, SettingSource, StoreState};
pub(crate) use store::{
    Proxied, can_change, metrics_store, proxy_get, proxy_refusal, read_stored, write_key,
};

pub(crate) mod store {
    use axum::Json;
    use axum::extract::State;
    use serde::{Deserialize, Serialize};
    use serde_json::{Value, json};
    use utoipa::ToSchema;
    use zeabur_core::problem::{Denied, Kind, KubernetesStatus, MEDIA_TYPE as PROBLEM, Problem};
    use zeabur_kube::{
        Cluster, ClusterError, ClusterExt as _, ObjectTarget, ServicePort, Unreadable, WriteBody,
        WriteRequest,
    };
    use zeabur_model::Origin;

    use super::{CONFIG_MAP, METRICS_STORE_KEY, Settings, parse_service_port};
    use crate::error::{ApiError, read_problem};
    use crate::handlers::{AppState, Person};
    use crate::projects::namespace::Permission;
    use crate::views::UnreadableView;

    /// Where the metrics store in use comes from.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
    #[serde(rename_all = "kebab-case")]
    pub enum SettingSource {
        /// What the server was started with.
        Default,
        /// A person's choice, kept in the settings ConfigMap.
        Setting,
    }

    /// Whether the metrics store answered, as the person. An open set.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
    #[serde(rename_all = "kebab-case")]
    pub enum StoreState {
        /// It answered.
        Available,
        /// There is no such Service, or it does not serve the Prometheus HTTP API (`404`).
        NotConfigured,
        /// The person may not reach it through the service proxy (`403`).
        Unreadable,
        /// It is there but did not answer: no ready pod (`503`), an error, or an answer that is
        /// not the API's.
        Unavailable,
    }

    /// The metrics store in use, as one request sees it.
    #[derive(Debug, Clone)]
    pub(crate) struct Store {
        pub(crate) service: ServicePort,
        pub(crate) source: SettingSource,
        /// The settings ConfigMap, when it could not be read.
        pub(crate) unreadable: Option<UnreadableView>,
        /// The settings ConfigMap's `resourceVersion` and data, when it exists.
        pub(crate) stored: Option<Value>,
    }

    fn config_map(namespace: &str) -> ObjectTarget {
        ObjectTarget {
            api_version: "v1".to_owned(),
            kind: "ConfigMap".to_owned(),
            plural: "configmaps".to_owned(),
            namespace: namespace.to_owned(),
            name: CONFIG_MAP.to_owned(),
        }
    }

    /// The settings ConfigMap as one person reads it.
    #[derive(Debug, Clone, Default)]
    pub(crate) struct Stored {
        /// The ConfigMap, when it exists and could be read.
        pub(crate) object: Option<Value>,
        /// The refusal, when it could not be read.
        pub(crate) unreadable: Option<UnreadableView>,
    }

    impl Stored {
        /// The text kept under `key`.
        pub(crate) fn get(&self, key: &str) -> Option<&str> {
            self.object
                .as_ref()?
                .pointer(&format!("/data/{key}"))
                .and_then(Value::as_str)
        }
    }

    /// The settings ConfigMap, read as the person; nothing where Zeabur keeps no settings. A
    /// refusal is kept to say so; another failure reads as no ConfigMap.
    pub(crate) async fn read_stored(cluster: &dyn Cluster, settings: &Settings) -> Stored {
        let Some(namespace) = &settings.namespace else {
            return Stored::default();
        };
        let target = config_map(namespace);
        let read = cluster
            .find_object(&target.resource(), target.namespace(), &target.name)
            .await
            .and_then(|found| {
                found
                    .map(serde_json::to_value)
                    .transpose()
                    .map_err(|e| ClusterError::Decode(e.to_string()))
            });
        match read {
            Ok(object) => Stored {
                object,
                unreadable: None,
            },
            Err(error) => Stored {
                object: None,
                unreadable: error.into_unreadable("configmaps").ok().map(Into::into),
            },
        }
    }

    /// The metrics store for this person: the setting when the ConfigMap holds a valid one, else
    /// the default. A refused or failed read of the ConfigMap falls back to the default too, and
    /// a refusal is kept to say so.
    pub(crate) async fn metrics_store(cluster: &dyn Cluster, settings: &Settings) -> Store {
        let stored = read_stored(cluster, settings).await;
        let chosen = stored
            .get(METRICS_STORE_KEY)
            .and_then(|text| parse_service_port(text).ok());
        Store {
            source: if chosen.is_some() {
                SettingSource::Setting
            } else {
                SettingSource::Default
            },
            service: chosen.unwrap_or_else(|| settings.metrics_store.clone()),
            unreadable: stored.unreadable,
            stored: stored.object,
        }
    }

    /// Whether the person may write the settings ConfigMap (`patch` it, or `create` it when
    /// `exists` is false), as far as their RBAC rules in Zeabur's namespace tell.
    pub(crate) async fn can_change(
        cluster: &dyn Cluster,
        settings: &Settings,
        exists: bool,
    ) -> Permission {
        let Some(namespace) = &settings.namespace else {
            return Permission::Denied;
        };
        let resource = zeabur_kube::ResourceRef::new("", "v1", "configmaps", "ConfigMap");
        let verb = if exists { "patch" } else { "create" };
        cluster
            .access_rules(namespace)
            .await
            .map_or(Permission::Unknown, |rules| {
                rules.allows(verb, &resource, CONFIG_MAP).into()
            })
    }

    /// Keeps `value` under `key` in the settings ConfigMap of `namespace` (removes the key for
    /// `None`), as the person, from the ConfigMap as `stored` read it.
    pub(crate) async fn write_key(
        cluster: &dyn Cluster,
        namespace: &str,
        stored: Option<&Value>,
        key: &str,
        value: Option<&str>,
    ) -> Result<(), ApiError> {
        let Some(body) = write_body(stored, key, value) else {
            return Ok(());
        };
        let verb = match &body {
            WriteBody::Create(_) => "create",
            WriteBody::JsonPatch(_) | WriteBody::Delete(_) | WriteBody::EphemeralContainers(_) => {
                "patch"
            }
        };
        let request = WriteRequest {
            target: config_map(namespace),
            body,
            dry_run: false,
        };
        cluster.write(&request).await.map(|_| ()).map_err(|error| {
            // The `test` of the version read failed: someone changed the settings meanwhile.
            if matches!(&error, ClusterError::Status(status) if status.code == 422) {
                return ApiError::from(Problem::new(
                    Kind::Conflict,
                    "Someone changed the settings after you loaded them. Load them again, then \
                     make your change.",
                ));
            }
            let refused = Denied {
                verb: verb.to_owned(),
                group: String::new(),
                resource: "configmaps".to_owned(),
                name: (verb == "patch").then(|| CONFIG_MAP.to_owned()),
                namespace: Some(namespace.to_owned()),
            };
            read_problem(error, "the settings ConfigMap", refused)
        })
    }

    /// What a `GET` through the proxy gave: the answer, or the state it means and kube-apiserver's
    /// (or the store's) words.
    pub(crate) enum Proxied {
        Answer(Value),
        Failed {
            state: StoreState,
            status: Option<KubernetesStatus>,
            message: String,
        },
    }

    /// `GET path_and_query` of the store, as the person.
    pub(crate) async fn proxy_get(
        cluster: &dyn Cluster,
        service: &ServicePort,
        path_and_query: &str,
    ) -> Proxied {
        match cluster.proxy_get(service, path_and_query).await {
            Ok(answer) => Proxied::Answer(answer),
            Err(ClusterError::Status(status)) => Proxied::Failed {
                state: if status.is_refusal() {
                    StoreState::Unreadable
                } else if status.is_not_found() {
                    StoreState::NotConfigured
                } else {
                    StoreState::Unavailable
                },
                message: status.message.clone(),
                status: Some(status),
            },
            Err(ClusterError::Transport(message) | ClusterError::Decode(message)) => {
                Proxied::Failed {
                    state: StoreState::Unavailable,
                    status: None,
                    message,
                }
            }
            // A store whose Service name no object can have: there is no such store.
            Err(ClusterError::InvalidName(message)) => Proxied::Failed {
                state: StoreState::NotConfigured,
                status: None,
                message,
            },
        }
    }

    /// Whether the store answers its probe (`/api/v1/status/buildinfo`), and the refusal or the
    /// words of a failure.
    pub(crate) async fn probe(
        cluster: &dyn Cluster,
        service: &ServicePort,
    ) -> (StoreState, Option<UnreadableView>, Option<String>) {
        let path = zeabur_modules::prometheus_api::PROMETHEUS_HTTP.probe;
        match proxy_get(cluster, service, path).await {
            Proxied::Answer(answer)
                if answer.get("status").and_then(Value::as_str) == Some("success") =>
            {
                (StoreState::Available, None, None)
            }
            Proxied::Answer(_) => (
                StoreState::Unavailable,
                None,
                Some("The Service answered, but not as the Prometheus HTTP API.".to_owned()),
            ),
            Proxied::Failed {
                state,
                status,
                message,
            } => {
                let refusal = (state == StoreState::Unreadable)
                    .then_some(status)
                    .flatten()
                    .map(|status| proxy_refusal(service, status));
                (state, refusal, Some(message))
            }
        }
    }

    /// A refused proxy read, as an unreadable entry naming the Service.
    pub(crate) fn proxy_refusal(service: &ServicePort, status: KubernetesStatus) -> UnreadableView {
        UnreadableView::from(Unreadable {
            resource: format!("services/proxy ({}/{})", service.namespace, service.name),
            status,
        })
    }

    /// The metrics store setting.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
    #[serde(rename_all = "camelCase")]
    pub struct MetricsStoreSetting {
        /// `settings/metrics-store`.
        pub id: String,
        /// The Service port in use, `namespace/name:port`.
        pub service: String,
        /// What the server was started with (`ZEABUR_METRICS_SERVICE`).
        pub default: String,
        /// Where `service` comes from.
        pub source: SettingSource,
        /// The ConfigMap a change is kept in; absent where Zeabur keeps no settings (on a
        /// person's computer).
        #[serde(skip_serializing_if = "Option::is_none")]
        pub stored_in: Option<Origin>,
        /// Whether the person may change it, as far as their RBAC rules in that namespace tell.
        pub can_change: Permission,
        /// Whether the store answers, as the person.
        pub state: StoreState,
        /// kube-apiserver's or the store's words when it does not.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub message: Option<String>,
        /// What could not be read: the settings ConfigMap, or the store through the proxy.
        pub unreadable: Vec<UnreadableView>,
    }

    /// A change of the metrics store.
    #[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    pub struct MetricsStoreChange {
        /// The Service port, `namespace/name:port`; `null` goes back to the default.
        pub service: Option<String>,
    }

    /// The setting as `cluster` sees it, `store` already read.
    async fn view(cluster: &dyn Cluster, settings: &Settings, store: Store) -> MetricsStoreSetting {
        let (state, refused, message) = probe(cluster, &store.service).await;
        let can_change = can_change(cluster, settings, store.stored.is_some()).await;
        MetricsStoreSetting {
            id: "settings/metrics-store".to_owned(),
            service: store.service.to_string(),
            default: settings.metrics_store.to_string(),
            source: store.source,
            stored_in: settings
                .namespace
                .as_deref()
                .map(|namespace| Origin::complete("v1", "ConfigMap", Some(namespace), CONFIG_MAP)),
            can_change,
            state,
            message,
            unreadable: store.unreadable.into_iter().chain(refused).collect(),
        }
    }

    /// The metrics store: which Service history is read from, where that came from, whether it
    /// answers and whether you may change it. Readable by everyone signed in; the store answers
    /// only those allowed `get` on its `services/proxy`.
    #[utoipa::path(get, path = "/api/v1/settings/metrics-store", tag = "settings",
        operation_id = "getMetricsStore",
        responses((status = 200, body = MetricsStoreSetting),
                  (status = 401, body = Problem, content_type = PROBLEM)))]
    pub(crate) async fn get_setting(
        State(state): State<AppState>,
        Person(cluster): Person,
    ) -> Json<MetricsStoreSetting> {
        let store = metrics_store(cluster.as_ref(), &state.settings).await;
        Json(view(cluster.as_ref(), &state.settings, store).await)
    }

    /// Changes the metrics store, or goes back to the default with `null`, by writing the
    /// settings ConfigMap as you (created the first time). Who may: whoever may update (and
    /// create) the ConfigMap `zeabur-settings` in Zeabur's namespace. Answers the setting as it
    /// now is, with the new store's probe.
    #[utoipa::path(put, path = "/api/v1/settings/metrics-store", tag = "settings",
        operation_id = "setMetricsStore",
        request_body = MetricsStoreChange,
        responses((status = 200, body = MetricsStoreSetting),
                  (status = 400, body = Problem, content_type = PROBLEM),
                  (status = 403, body = Problem, content_type = PROBLEM),
                  (status = 409, body = Problem, content_type = PROBLEM)))]
    pub(crate) async fn set(
        State(state): State<AppState>,
        Person(cluster): Person,
        Json(change): Json<MetricsStoreChange>,
    ) -> Result<Json<MetricsStoreSetting>, ApiError> {
        let Some(namespace) = state.settings.namespace.clone() else {
            return Err(ApiError::from(Problem::new(
                Kind::BadRequest,
                "Zeabur runs on this computer and keeps no settings. To read history from \
                 another store, start it with --metrics-service namespace/name:port.",
            )));
        };
        let service = change
            .service
            .as_deref()
            .map(parse_service_port)
            .transpose()
            .map_err(|detail| ApiError::from(Problem::new(Kind::BadRequest, detail)))?;
        let store = metrics_store(cluster.as_ref(), &state.settings).await;
        let wanted = service.as_ref().map(ToString::to_string);
        write_key(
            cluster.as_ref(),
            &namespace,
            store.stored.as_ref(),
            METRICS_STORE_KEY,
            wanted.as_deref(),
        )
        .await?;
        let store = metrics_store(cluster.as_ref(), &state.settings).await;
        Ok(Json(view(cluster.as_ref(), &state.settings, store).await))
    }

    /// The write that makes the ConfigMap hold `value` under `key` (or nothing, for the
    /// default); `None` when it already does. A patch tests the `resourceVersion` it was read at,
    /// so a change made in between is refused (`409`) rather than overwritten.
    fn write_body(stored: Option<&Value>, key: &str, value: Option<&str>) -> Option<WriteBody> {
        let pointer = format!("/data/{key}");
        let Some(stored) = stored else {
            let value = value?;
            return Some(WriteBody::Create(json!({
                "apiVersion": "v1", "kind": "ConfigMap",
                "metadata": {"name": CONFIG_MAP},
                "data": {key: value}
            })));
        };
        let current = stored.pointer(&pointer).and_then(Value::as_str);
        let wanted = value.map(str::to_owned);
        if current == wanted.as_deref() {
            return None;
        }
        let version = stored
            .pointer("/metadata/resourceVersion")
            .cloned()
            .unwrap_or(Value::Null);
        let test = json!({"op": "test", "path": "/metadata/resourceVersion", "value": version});
        let change = match (wanted, stored.get("data").is_some_and(Value::is_object)) {
            (Some(wanted), true) => json!({"op": "add", "path": pointer, "value": wanted}),
            (Some(wanted), false) => {
                json!({"op": "add", "path": "/data", "value": {key: wanted}})
            }
            (None, _) => json!({"op": "remove", "path": pointer}),
        };
        Some(WriteBody::JsonPatch(json!([test, change])))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn service() -> ServicePort {
            super::super::default_metrics_store()
        }

        #[test]
        fn the_first_choice_creates_the_config_map() {
            let Some(WriteBody::Create(body)) =
                write_body(None, METRICS_STORE_KEY, Some(&service().to_string()))
            else {
                panic!("not a create");
            };
            assert_eq!(
                body["data"][METRICS_STORE_KEY],
                "observability/vmsingle-vm-victoria-metrics-k8s-stack:8428"
            );
            assert_eq!(write_body(None, METRICS_STORE_KEY, None), None);
        }

        #[test]
        fn a_change_tests_the_version_it_was_read_at() {
            let stored = json!({"metadata": {"resourceVersion": "7"}, "data": {"other": "x"}});
            let Some(WriteBody::JsonPatch(patch)) = write_body(
                Some(&stored),
                METRICS_STORE_KEY,
                Some(&service().to_string()),
            ) else {
                panic!("not a patch");
            };
            assert_eq!(
                patch[0],
                json!({"op": "test", "path": "/metadata/resourceVersion", "value": "7"})
            );
            assert_eq!(patch[1]["op"], "add");
            assert_eq!(patch[1]["path"], "/data/metrics-store");
            let empty = json!({"metadata": {"resourceVersion": "7"}});
            let Some(WriteBody::JsonPatch(patch)) = write_body(
                Some(&empty),
                METRICS_STORE_KEY,
                Some(&service().to_string()),
            ) else {
                panic!("not a patch");
            };
            assert_eq!(patch[1]["path"], "/data");
        }

        #[test]
        fn going_back_to_the_default_removes_the_key_once() {
            let stored = json!({"metadata": {"resourceVersion": "7"},
                "data": {METRICS_STORE_KEY: "a/b:80"}});
            let Some(WriteBody::JsonPatch(patch)) =
                write_body(Some(&stored), METRICS_STORE_KEY, None)
            else {
                panic!("not a patch");
            };
            assert_eq!(
                patch[1],
                json!({"op": "remove", "path": "/data/metrics-store"})
            );
            let unchanged = json!({"metadata": {"resourceVersion": "7"}, "data": {}});
            assert_eq!(write_body(Some(&unchanged), METRICS_STORE_KEY, None), None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_service_port_is_namespace_name_and_port() {
        let port = parse_service_port("monitoring/prometheus-operated:web").unwrap();
        assert_eq!(
            (
                port.namespace.as_str(),
                port.name.as_str(),
                port.port.as_str()
            ),
            ("monitoring", "prometheus-operated", "web")
        );
        assert_eq!(port.to_string(), "monitoring/prometheus-operated:web");
        assert!(parse_service_port(" observability/vm:8428 ").is_ok());
        for bad in [
            "vm:8428",
            "observability/vm",
            "Observability/vm:8428",
            "observability/vm:0",
            "observability/vm:70000",
            "observability/vm:-web",
            "observability/vm:1234567890123456",
            "observability/vm/x:80",
            "a/b:80/../c",
        ] {
            assert!(parse_service_port(bad).is_err(), "{bad}");
        }
    }
}
