//! The [`Cluster`] port against kube-apiserver, through kube-rs, as the person a
//! [`UserClient`] is bound to.
//!
//! Secrets are listed through kube-apiserver's Table rendering with `includeObject=Metadata`,
//! the form `kubectl get secrets` prints [k8s-table]: name, type and key count, never the
//! values. Values are read only by [`Cluster::reveal_secret`].
//!
//! [k8s-table]: https://kubernetes.io/docs/reference/using-api/api-concepts/#receiving-resources-as-tables

use std::collections::{BTreeMap, BTreeSet};

use futures_util::{AsyncBufReadExt as _, TryStreamExt as _};
use k8s_openapi::api::apps::v1::{DaemonSet, Deployment, StatefulSet};
use k8s_openapi::api::authentication::v1::SelfSubjectReview;
use k8s_openapi::api::batch::v1::{CronJob, Job};
use k8s_openapi::api::core::v1::{
    ConfigMap, Namespace, Node, PersistentVolumeClaim, Pod, Secret, Service,
};
use k8s_openapi::api::networking::v1::{Ingress, IngressClass};
use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, Status};
use k8s_openapi::kube_aggregator::pkg::apis::apiregistration::v1::APIService;
use kube::api::{Api, AttachParams, DynamicObject, ListParams, LogParams, PostParams};
use kube::client::UpgradeConnectionError;
use kube::core::{ApiResource, GroupVersionKind};
use serde::de::DeserializeOwned;
use zeabur_core::problem::{Denied, FieldCause, KubernetesStatus, StatusDetails};

use crate::objects::{self, AccessRules, ResourceRef, ServedResource};
use crate::port::{
    Answer, Cluster, ClusterError, ClusterEvents, ClusterFacts, ClusterPods, ExecRequest,
    ExecSession, ExecStream, ExitStatus, Identity, LogLines, LogOptions, NamespaceObjects,
    SecretSummary, ServicePort, TerminalSize, Unreadable, WriteRequest, Written,
};
use crate::{UserClient, names, wide};

/// The buffer of each of [`Cluster::stream`]'s pipes: large enough that a file moves in
/// WebSocket frames of tens of kilobytes rather than kube's default kilobyte.
const STREAM_BUFFER: usize = 64 * 1024;

impl From<kube::Error> for ClusterError {
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "kube::Error's variants depend on kube's features; every other one is transport"
    )]
    fn from(error: kube::Error) -> Self {
        match error {
            kube::Error::Api(status) => Self::Status(KubernetesStatus {
                retry_after_seconds: status
                    .details
                    .as_ref()
                    .map(|d| d.retry_after_seconds)
                    .filter(|&s| s > 0),
                names_invalid_field: status
                    .details
                    .as_ref()
                    .is_some_and(|d| !d.name.is_empty() || !d.causes.is_empty()),
                details: status
                    .details
                    .as_ref()
                    .map(|d| StatusDetails {
                        object: (!d.kind.is_empty() && !d.name.is_empty())
                            .then(|| (d.kind.clone(), d.name.clone())),
                        causes: d
                            .causes
                            .iter()
                            .filter(|cause| !cause.field.is_empty())
                            .map(|cause| FieldCause {
                                field: cause.field.clone(),
                                reason: cause.reason.clone(),
                                message: cause.message.clone(),
                            })
                            .collect(),
                    })
                    .filter(|d| *d != StatusDetails::default())
                    .map(Box::new),
                ..KubernetesStatus::new(status.code, status.reason.clone(), status.message.clone())
            }),
            kube::Error::SerdeError(e) => Self::Decode(e.to_string()),
            kube::Error::UpgradeConnection(UpgradeConnectionError::ProtocolSwitch(code)) => {
                refused_upgrade(code)
            }
            other => Self::Transport(other.to_string()),
        }
    }
}

/// The outcome of listing one kind: the objects, nothing (not installed), or the refusal.
enum Listed<T> {
    Items(Vec<T>),
    Refused(Unreadable),
}

/// Lists one kind; a `404` (the API is not installed) is an empty list, a refusal is recorded.
async fn list_kind<T>(api: Api<T>, resource: &str) -> Result<Listed<T>, ClusterError>
where
    T: kube::Resource + Clone + DeserializeOwned + std::fmt::Debug,
{
    match api.list(&ListParams::default()).await {
        Ok(list) => Ok(Listed::Items(list.items)),
        Err(error) => refusal(error, resource),
    }
}

/// Lists only the metadata of one kind, for counting and labels.
async fn list_metadata<T>(api: Api<T>, resource: &str) -> Result<Listed<ObjectMeta>, ClusterError>
where
    T: kube::Resource + Clone + DeserializeOwned + std::fmt::Debug,
{
    match api.list_metadata(&ListParams::default()).await {
        Ok(list) => Ok(Listed::Items(
            list.items.into_iter().map(|o| o.metadata).collect(),
        )),
        Err(error) => refusal(error, resource),
    }
}

fn refusal<T>(error: kube::Error, resource: &str) -> Result<Listed<T>, ClusterError> {
    let error = ClusterError::from(error);
    if error.is_not_found() {
        return Ok(Listed::Items(Vec::new()));
    }
    error.into_unreadable(resource).map(Listed::Refused)
}

/// Collects listed items into `into`, refusals into `unreadable`; counts what was readable.
fn take<T>(listed: Listed<T>, into: &mut Vec<T>, unreadable: &mut Vec<Unreadable>) -> usize {
    match listed {
        Listed::Items(items) => {
            *into = items;
            1
        }
        Listed::Refused(refused) => {
            unreadable.push(refused);
            0
        }
    }
}

/// How an exec ended, from the `Status` the kubelet sends on the stream's error channel:
/// `Success`, or `Failure` with reason `NonZeroExitCode` and the code in a cause of reason
/// `ExitCode` [kubelet-remotecommand]. Any other failure is the runtime's: a command missing
/// from the image is told by runc's message (`stat /bin/sh: no such file or directory`) or Go's
/// `exec.LookPath` (`executable file not found in $PATH`), since the runtime gives it no code.
/// No status at all means the stream broke.
///
/// [kubelet-remotecommand]: https://github.com/kubernetes/kubernetes/blob/master/staging/src/k8s.io/kubelet/pkg/cri/streaming/remotecommand/httpstream.go
pub(crate) fn exit_status(status: Option<Status>) -> ExitStatus {
    let Some(status) = status else {
        return ExitStatus::Failed("the stream to the container ended".to_owned());
    };
    if status.status.as_deref() == Some("Success") {
        return ExitStatus::Exited(0);
    }
    if status.reason.as_deref() == Some("NonZeroExitCode") {
        let code = status
            .details
            .iter()
            .flat_map(|d| d.causes.iter().flatten())
            .find(|c| c.reason.as_deref() == Some("ExitCode"))
            .and_then(|c| c.message.as_deref()?.parse().ok());
        if let Some(code) = code {
            return ExitStatus::Exited(code);
        }
    }
    let message = status.message.unwrap_or_default();
    if message.contains("no such file or directory")
        || message.contains("executable file not found")
    {
        ExitStatus::CommandNotFound
    } else {
        ExitStatus::Failed(message)
    }
}

/// kube-rs reports a refused exec upgrade with the HTTP status only (the `Status` body is not
/// read), so this is the closest answer: the code, and its reason phrase as the reason.
fn refused_upgrade(code: http::StatusCode) -> ClusterError {
    let reason: String = code
        .canonical_reason()
        .unwrap_or_default()
        .split_whitespace()
        .collect();
    ClusterError::Status(KubernetesStatus::new(
        code.as_u16(),
        reason,
        format!("kube-apiserver refused to open the stream ({code})"),
    ))
}

fn dynamic(group: &str, version: &str, kind: &str, plural: &str) -> ApiResource {
    ApiResource::from_gvk_with_plural(&GroupVersionKind::gvk(group, version, kind), plural)
}

/// Secrets of `namespace` without their values, through the Table rendering.
async fn secret_summaries(
    client: &kube::Client,
    namespace: &str,
) -> Result<Listed<SecretSummary>, ClusterError> {
    let request = http::Request::get(format!(
        "/api/v1/namespaces/{namespace}/secrets?includeObject=Metadata"
    ))
    .header(
        http::header::ACCEPT,
        "application/json;as=Table;v=v1;g=meta.k8s.io",
    )
    .body(Vec::new())
    .map_err(|e| ClusterError::Transport(e.to_string()))?;
    match client.request::<serde_json::Value>(request).await {
        Ok(table) => parse_secret_table(&table).map(Listed::Items),
        Err(error) => refusal(error, "secrets"),
    }
}

/// Reads a `meta.k8s.io/v1` Table of Secrets: the `Type` and `Data` columns and each row's
/// metadata, with the last-applied annotation hidden ([`crate::objects`]), since it may hold
/// the values. Anything else in the answer is ignored.
fn parse_secret_table(table: &serde_json::Value) -> Result<Vec<SecretSummary>, ClusterError> {
    let columns: Vec<&str> = table
        .get("columnDefinitions")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| ClusterError::Decode("secret table has no columns".to_owned()))?
        .iter()
        .map(|c| {
            c.get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
        })
        .collect();
    let column = |name: &str| columns.iter().position(|c| *c == name);
    let (type_column, data_column) = (column("Type"), column("Data"));
    let rows = table
        .get("rows")
        .and_then(serde_json::Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    rows.iter()
        .map(|row| {
            let cells = row.get("cells").and_then(serde_json::Value::as_array);
            let cell = |index: Option<usize>| cells.zip(index).and_then(|(c, i)| c.get(i));
            let mut metadata: ObjectMeta = row
                .pointer("/object/metadata")
                .cloned()
                .map(serde_json::from_value)
                .transpose()
                .map_err(|e| ClusterError::Decode(e.to_string()))?
                .unwrap_or_default();
            // `kubectl apply` keeps a copy of the whole Secret, values included, in an
            // annotation that the metadata carries.
            objects::hide_last_applied(&mut metadata);
            Ok(SecretSummary {
                secret: Secret {
                    metadata,
                    type_: cell(type_column)
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned),
                    ..Secret::default()
                },
                key_count: cell(data_column)
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|n| usize::try_from(n).ok())
                    .unwrap_or_default(),
            })
        })
        .collect()
}

fn log_params(options: &LogOptions, follow: bool) -> LogParams {
    LogParams {
        container: options.container.clone(),
        follow,
        tail_lines: options.tail_lines,
        since_seconds: options.since_seconds,
        previous: options.previous,
        timestamps: options.timestamps,
        ..LogParams::default()
    }
}

/// The five workload kinds and pods of `namespace`, concurrently, with how many could be read.
async fn workloads(
    client: &kube::Client,
    namespace: &str,
) -> Result<(NamespaceObjects, usize), ClusterError> {
    let ns = |c: &kube::Client| c.clone();
    let (deployments, stateful_sets, daemon_sets, jobs, cron_jobs, pods) = futures_util::try_join!(
        list_kind(
            Api::<Deployment>::namespaced(ns(client), namespace),
            "deployments.apps"
        ),
        list_kind(
            Api::<StatefulSet>::namespaced(ns(client), namespace),
            "statefulsets.apps"
        ),
        list_kind(
            Api::<DaemonSet>::namespaced(ns(client), namespace),
            "daemonsets.apps"
        ),
        list_kind(Api::<Job>::namespaced(ns(client), namespace), "jobs.batch"),
        list_kind(
            Api::<CronJob>::namespaced(ns(client), namespace),
            "cronjobs.batch"
        ),
        list_kind(Api::<Pod>::namespaced(ns(client), namespace), "pods"),
    )?;
    let mut objects = NamespaceObjects {
        namespace: namespace.to_owned(),
        ..NamespaceObjects::default()
    };
    let u = &mut objects.unreadable;
    let mut readable = 0;
    readable += take(deployments, &mut objects.deployments, u);
    readable += take(stateful_sets, &mut objects.stateful_sets, u);
    readable += take(daemon_sets, &mut objects.daemon_sets, u);
    readable += take(jobs, &mut objects.jobs, u);
    readable += take(cron_jobs, &mut objects.cron_jobs, u);
    readable += take(pods, &mut objects.pods, u);
    Ok((objects, readable))
}

/// `objects`, unless nothing at all could be read: then kube-apiserver's first refusal.
fn all_refused(
    objects: NamespaceObjects,
    readable: usize,
) -> Result<NamespaceObjects, ClusterError> {
    if readable == 0 {
        return Err(objects.unreadable.into_iter().next().map_or_else(
            || ClusterError::Decode("no kinds".to_owned()),
            |u| ClusterError::Status(u.status),
        ));
    }
    Ok(objects)
}

impl Cluster for UserClient {
    fn whoami(&self) -> Answer<'_, Identity> {
        Box::pin(async move {
            let review = Api::<SelfSubjectReview>::all(self.kube().clone())
                .create(&PostParams::default(), &SelfSubjectReview::default())
                .await?;
            let user = review.status.and_then(|s| s.user_info).unwrap_or_default();
            Ok(Identity {
                username: user.username.unwrap_or_default(),
                groups: user.groups.unwrap_or_default(),
            })
        })
    }

    fn namespaces(&self) -> Answer<'_, Vec<Namespace>> {
        Box::pin(async move {
            Ok(Api::<Namespace>::all(self.kube().clone())
                .list(&ListParams::default())
                .await?
                .items)
        })
    }

    fn namespace_objects<'a>(&'a self, namespace: &'a str) -> Answer<'a, NamespaceObjects> {
        Box::pin(async move {
            names::namespace(namespace)?;
            let client = self.kube();
            let ns = |c: &kube::Client| c.clone();
            let http_routes = dynamic("gateway.networking.k8s.io", "v1", "HTTPRoute", "httproutes");
            let (workloads, services, config_maps, secrets, claims, ingresses, routes) = futures_util::try_join!(
                workloads(client, namespace),
                list_kind(
                    Api::<Service>::namespaced(ns(client), namespace),
                    "services"
                ),
                list_kind(
                    Api::<ConfigMap>::namespaced(ns(client), namespace),
                    "configmaps"
                ),
                secret_summaries(client, namespace),
                list_kind(
                    Api::<PersistentVolumeClaim>::namespaced(ns(client), namespace),
                    "persistentvolumeclaims"
                ),
                list_kind(
                    Api::<Ingress>::namespaced(ns(client), namespace),
                    "ingresses.networking.k8s.io"
                ),
                list_kind(
                    Api::<DynamicObject>::namespaced_with(ns(client), namespace, &http_routes),
                    "httproutes.gateway.networking.k8s.io"
                ),
            )?;

            let (mut objects, mut readable) = workloads;
            let u = &mut objects.unreadable;
            readable += take(services, &mut objects.services, u);
            readable += take(config_maps, &mut objects.config_maps, u);
            readable += take(secrets, &mut objects.secrets, u);
            readable += take(claims, &mut objects.persistent_volume_claims, u);
            readable += take(ingresses, &mut objects.ingresses, u);
            let mut route_objects = Vec::new();
            readable += take(routes, &mut route_objects, u);
            objects.http_routes = route_objects
                .into_iter()
                .map(serde_json::to_value)
                .collect::<Result<_, _>>()
                .map_err(|e| ClusterError::Decode(e.to_string()))?;
            all_refused(objects, readable)
        })
    }

    fn namespace_workloads<'a>(&'a self, namespace: &'a str) -> Answer<'a, NamespaceObjects> {
        Box::pin(async move {
            names::namespace(namespace)?;
            let (objects, readable) = workloads(self.kube(), namespace).await?;
            all_refused(objects, readable)
        })
    }

    fn version(&self) -> Answer<'_, String> {
        Box::pin(async move { Ok(self.kube().apiserver_version().await?.git_version) })
    }

    fn readyz(&self) -> Answer<'_, String> {
        Box::pin(async move {
            let request = http::Request::get("/readyz?verbose")
                .body(Vec::new())
                .map_err(|e| ClusterError::Transport(e.to_string()))?;
            match self.kube().request_text(request).await {
                Ok(body) => Ok(body),
                // A server that is not ready answers `500` with the same plain-text list, which
                // kube reports as a status it could not parse, carrying the body as its message.
                Err(kube::Error::Api(status))
                    if status.code == 500 && status.message.contains("[-]") =>
                {
                    Ok(status.message.clone())
                }
                Err(error) => Err(error.into()),
            }
        })
    }

    fn kubeconfig_name(&self) -> Option<String> {
        self.name.clone()
    }

    fn pod_logs<'a>(
        &'a self,
        namespace: &'a str,
        pod: &'a str,
        options: &'a LogOptions,
    ) -> Answer<'a, String> {
        Box::pin(async move {
            names::namespace(namespace)?;
            names::name(pod)?;
            Ok(Api::<Pod>::namespaced(self.kube().clone(), namespace)
                .logs(pod, &log_params(options, false))
                .await?)
        })
    }

    fn follow_pod_logs<'a>(
        &'a self,
        namespace: &'a str,
        pod: &'a str,
        options: &'a LogOptions,
    ) -> Answer<'a, LogLines> {
        Box::pin(async move {
            names::namespace(namespace)?;
            names::name(pod)?;
            let reader = Api::<Pod>::namespaced(self.kube().clone(), namespace)
                .log_stream(pod, &log_params(options, true))
                .await?;
            let lines: LogLines = Box::pin(
                reader
                    .lines()
                    .map_err(|e| ClusterError::Transport(e.to_string())),
            );
            Ok(lines)
        })
    }

    fn exec<'a>(
        &'a self,
        namespace: &'a str,
        pod: &'a str,
        request: &'a ExecRequest,
    ) -> Answer<'a, ExecSession> {
        Box::pin(async move {
            names::namespace(namespace)?;
            names::name(pod)?;
            let params = AttachParams::interactive_tty().container(&request.container);
            let mut process = Api::<Pod>::namespaced(self.kube().clone(), namespace)
                .exec(pod, request.command.clone(), &params)
                .await?;
            let missing =
                |what: &str| ClusterError::Decode(format!("the exec stream has no {what}"));
            let input = process.stdin().ok_or_else(|| missing("stdin"))?;
            let output = process.stdout().ok_or_else(|| missing("stdout"))?;
            let mut sizes = process.terminal_size().ok_or_else(|| missing("TTY"))?;
            let status = process.take_status().ok_or_else(|| missing("status"))?;
            Ok(ExecSession {
                input: Box::pin(input),
                output: Box::pin(output),
                resize: Box::new(move |size: TerminalSize| {
                    // A full queue (ten pending sizes) drops this one; the next resize brings the
                    // terminal up to date.
                    let _dropped = sizes.try_send(kube::api::TerminalSize {
                        width: size.cols,
                        height: size.rows,
                    });
                }),
                // The process lives inside this future: dropping it aborts kube's stream task.
                exit: Box::pin(async move {
                    let status = status.await;
                    drop(process);
                    exit_status(status)
                }),
            })
        })
    }

    /// Closing standard input reaches the process as end of file through the `v5.channel.k8s.io`
    /// close signal [kep-4006], which kube sends when the writer is dropped.
    ///
    /// [kep-4006]: https://github.com/kubernetes/enhancements/blob/master/keps/sig-api-machinery/4006-transition-spdy-to-websockets/README.md
    fn run<'a>(
        &'a self,
        namespace: &'a str,
        pod: &'a str,
        container: &'a str,
        command: &'a [String],
        input: &'a [u8],
    ) -> Answer<'a, ExitStatus> {
        Box::pin(async move {
            use tokio::io::AsyncWriteExt as _;
            names::namespace(namespace)?;
            names::name(pod)?;
            let params = AttachParams::default()
                .container(container)
                .stdin(true)
                .stdout(false)
                .stderr(false);
            let mut process = Api::<Pod>::namespaced(self.kube().clone(), namespace)
                .exec(pod, command.to_vec(), &params)
                .await?;
            let status = process
                .take_status()
                .ok_or_else(|| ClusterError::Decode("the exec stream has no status".to_owned()))?;
            let mut stdin = process
                .stdin()
                .ok_or_else(|| ClusterError::Decode("the exec stream has no stdin".to_owned()))?;
            stdin
                .write_all(input)
                .await
                .map_err(|e| ClusterError::Transport(e.to_string()))?;
            drop(stdin);
            let status = status.await;
            drop(process);
            Ok(exit_status(status))
        })
    }

    fn stream<'a>(
        &'a self,
        namespace: &'a str,
        pod: &'a str,
        container: &'a str,
        command: &'a [String],
    ) -> Answer<'a, ExecStream> {
        Box::pin(async move {
            names::namespace(namespace)?;
            names::name(pod)?;
            let params = AttachParams::default()
                .container(container)
                .stdin(true)
                .stdout(true)
                .stderr(true)
                .max_stdin_buf_size(STREAM_BUFFER)
                .max_stdout_buf_size(STREAM_BUFFER)
                .max_stderr_buf_size(STREAM_BUFFER);
            let mut process = Api::<Pod>::namespaced(self.kube().clone(), namespace)
                .exec(pod, command.to_vec(), &params)
                .await?;
            let missing =
                |what: &str| ClusterError::Decode(format!("the exec stream has no {what}"));
            let input = process.stdin().ok_or_else(|| missing("stdin"))?;
            let output = process.stdout().ok_or_else(|| missing("stdout"))?;
            let error = process.stderr().ok_or_else(|| missing("stderr"))?;
            let status = process.take_status().ok_or_else(|| missing("status"))?;
            Ok(ExecStream {
                input: Box::pin(input),
                output: Box::pin(output),
                error: Box::pin(error),
                exit: Box::pin(async move {
                    let status = status.await;
                    drop(process);
                    exit_status(status)
                }),
            })
        })
    }

    fn reveal_secret<'a>(
        &'a self,
        namespace: &'a str,
        name: &'a str,
    ) -> Answer<'a, BTreeMap<String, Vec<u8>>> {
        Box::pin(async move {
            names::namespace(namespace)?;
            names::name(name)?;
            let secret = Api::<Secret>::namespaced(self.kube().clone(), namespace)
                .get(name)
                .await?;
            let mut values: BTreeMap<String, Vec<u8>> = secret
                .data
                .unwrap_or_default()
                .into_iter()
                .map(|(k, v)| (k, v.0))
                .collect();
            for (k, v) in secret.string_data.unwrap_or_default() {
                values.insert(k, v.into_bytes());
            }
            Ok(values)
        })
    }

    fn cluster_facts(&self) -> Answer<'_, ClusterFacts> {
        Box::pin(async move {
            let client = self.kube();
            let c = || client.clone();
            let gateway_classes = dynamic(
                "gateway.networking.k8s.io",
                "v1",
                "GatewayClass",
                "gatewayclasses",
            );
            let helm_charts = dynamic("helm.cattle.io", "v1", "HelmChart", "helmcharts");
            let (version, nodes, crds, api_services, ingress_classes, gateways, k3s_charts) = futures_util::join!(
                client.apiserver_version(),
                list_metadata(Api::<Node>::all(c()), "nodes"),
                list_metadata(
                    Api::<CustomResourceDefinition>::all(c()),
                    "customresourcedefinitions.apiextensions.k8s.io"
                ),
                list_metadata(
                    Api::<APIService>::all(c()),
                    "apiservices.apiregistration.k8s.io"
                ),
                list_kind(
                    Api::<IngressClass>::all(c()),
                    "ingressclasses.networking.k8s.io"
                ),
                list_kind(
                    Api::<DynamicObject>::all_with(c(), &gateway_classes),
                    "gatewayclasses.gateway.networking.k8s.io"
                ),
                list_kind(
                    Api::<DynamicObject>::all_with(c(), &helm_charts),
                    "helmcharts.helm.cattle.io"
                ),
            );

            let mut facts = ClusterFacts {
                version: version.map(|v| v.git_version).map_err(ClusterError::from)?,
                ..ClusterFacts::default()
            };
            let mut metas = Vec::new();
            if take(nodes?, &mut metas, &mut facts.unreadable) == 1 {
                facts.nodes = Some(metas.len());
            }
            let names = |metas: Vec<ObjectMeta>| -> BTreeSet<String> {
                metas.into_iter().filter_map(|m| m.name).collect()
            };
            let mut metas = Vec::new();
            take(crds?, &mut metas, &mut facts.unreadable);
            facts.crds = names(metas);
            let mut metas = Vec::new();
            take(api_services?, &mut metas, &mut facts.unreadable);
            facts.api_services = names(metas);
            let mut classes = Vec::new();
            take(ingress_classes?, &mut classes, &mut facts.unreadable);
            facts.ingress_controllers = classes
                .into_iter()
                .filter_map(|c| c.spec.and_then(|s| s.controller))
                .collect();
            let mut classes = Vec::new();
            take(gateways?, &mut classes, &mut facts.unreadable);
            facts.gateway_controllers = classes
                .into_iter()
                .filter_map(|c| {
                    c.data
                        .pointer("/spec/controllerName")
                        .and_then(|v| v.as_str())
                        .map(str::to_owned)
                })
                .collect();
            let mut charts = Vec::new();
            take(k3s_charts?, &mut charts, &mut facts.unreadable);
            facts.k3s_helm_chart_versions = chart_versions(&charts);
            facts.k3s_helm_charts = charts
                .into_iter()
                .filter_map(|c| {
                    let key = format!(
                        "{}/{}",
                        c.metadata.namespace.as_deref().unwrap_or_default(),
                        c.metadata.name.as_deref().unwrap_or_default()
                    );
                    let chart = c
                        .data
                        .pointer("/spec/chart")
                        .and_then(|v| v.as_str())?
                        .to_owned();
                    Some((key, chart))
                })
                .collect();
            releases_and_workloads(client, &mut facts).await?;
            Ok(facts)
        })
    }

    fn cluster_pods(&self, limit: usize) -> Answer<'_, ClusterPods> {
        Box::pin(async move { wide::cluster_pods(self.kube(), limit).await })
    }

    fn cluster_events(&self, limit: usize) -> Answer<'_, ClusterEvents> {
        Box::pin(async move { wide::cluster_events(self.kube(), limit).await })
    }

    fn node_pods<'a>(&'a self, node: &'a str) -> Answer<'a, Vec<Pod>> {
        Box::pin(async move {
            names::name(node)?;
            let params = ListParams::default().fields(&format!("spec.nodeName={node}"));
            Ok(Api::<Pod>::all(self.kube().clone())
                .list(&params)
                .await?
                .items)
        })
    }

    fn serves<'a>(&'a self, group_version: &'a str) -> Answer<'a, Option<bool>> {
        Box::pin(async move { wide::serves(self.kube(), group_version).await })
    }

    fn served_resources(&self) -> Answer<'_, Vec<ServedResource>> {
        Box::pin(async move { objects::served_resources(self.kube()).await })
    }

    fn list_objects<'a>(
        &'a self,
        resource: &'a ResourceRef,
        namespace: Option<&'a str>,
    ) -> Answer<'a, Option<Vec<DynamicObject>>> {
        Box::pin(async move {
            names::optional_namespace(namespace)?;
            objects::list_objects(self.kube(), resource, namespace).await
        })
    }

    fn list_metadata<'a>(
        &'a self,
        resource: &'a ResourceRef,
        namespace: Option<&'a str>,
    ) -> Answer<'a, Option<Vec<ObjectMeta>>> {
        Box::pin(async move {
            names::optional_namespace(namespace)?;
            objects::list_metadata(self.kube(), resource, namespace).await
        })
    }

    fn list_served_metadata<'a>(
        &'a self,
        resource: &'a ResourceRef,
        namespace: &'a str,
    ) -> Answer<'a, Option<Vec<ObjectMeta>>> {
        Box::pin(async move {
            names::namespace(namespace)?;
            objects::list_served_metadata(self.kube(), resource, namespace).await
        })
    }

    fn access_rules<'a>(&'a self, namespace: &'a str) -> Answer<'a, AccessRules> {
        Box::pin(async move {
            names::namespace(namespace)?;
            objects::access_rules(self.kube(), namespace).await
        })
    }

    fn review_access<'a>(&'a self, request: &'a Denied) -> Answer<'a, bool> {
        Box::pin(async move { objects::review_access(self.kube(), request).await })
    }

    fn list_kind<'a>(
        &'a self,
        resource: &'a ResourceRef,
        namespace: Option<&'a str>,
    ) -> Answer<'a, Vec<DynamicObject>> {
        Box::pin(async move {
            names::optional_namespace(namespace)?;
            objects::list_kind(self.kube(), resource, namespace).await
        })
    }

    fn get_object<'a>(
        &'a self,
        resource: &'a ResourceRef,
        namespace: Option<&'a str>,
        name: &'a str,
    ) -> Answer<'a, Option<DynamicObject>> {
        Box::pin(async move {
            names::optional_namespace(namespace)?;
            names::name(name)?;
            objects::get_object(self.kube(), resource, namespace, name).await
        })
    }

    // ── Writes ───────────────────────────────────────────────────────────────────────────────

    fn write<'a>(&'a self, request: &'a WriteRequest) -> Answer<'a, Written> {
        Box::pin(async move { crate::write::write(self.kube(), request).await })
    }

    // ── Reads for releases, storage and YAML edits ───────────────────────────────────────────

    fn proxy_get<'a>(
        &'a self,
        service: &'a ServicePort,
        path_and_query: &'a str,
    ) -> Answer<'a, serde_json::Value> {
        Box::pin(async move {
            names::namespace(&service.namespace)?;
            names::name(&service.name)?;
            let request = http::Request::get(proxy_path(service, path_and_query)?)
                .header(http::header::ACCEPT, "application/json")
                .body(Vec::new())
                .map_err(|e| ClusterError::Transport(e.to_string()))?;
            Ok(self.kube().request::<serde_json::Value>(request).await?)
        })
    }
}

/// The service proxy's path for `path_and_query` of `service`. Refuses a path that does not
/// start with `/` or that climbs out of the proxy (`..`), so a mistake in Zeabur's own paths
/// cannot reach another of kube-apiserver's.
fn proxy_path(service: &ServicePort, path_and_query: &str) -> Result<String, ClusterError> {
    let path = path_and_query.split('?').next().unwrap_or_default();
    if !path.starts_with('/') || path.split('/').any(|segment| segment == "..") {
        return Err(ClusterError::Transport(format!(
            "{path_and_query:?} is not a path for the service proxy"
        )));
    }
    Ok(format!(
        "/api/v1/namespaces/{}/services/{}:{}/proxy{path_and_query}",
        service.namespace, service.name, service.port
    ))
}

/// The `spec.version` of k3s `HelmChart` objects that name one, by `namespace/name`.
fn chart_versions(charts: &[DynamicObject]) -> BTreeMap<String, String> {
    charts
        .iter()
        .filter_map(|c| {
            let version = c.data.pointer("/spec/version")?.as_str()?;
            Some((
                format!(
                    "{}/{}",
                    c.metadata.namespace.as_deref().unwrap_or_default(),
                    c.metadata.name.as_deref().unwrap_or_default()
                ),
                version.to_owned(),
            ))
        })
        .filter(|(_, version)| !version.is_empty())
        .collect()
}

/// Helm releases (from release Secrets' labels, never decoded) and workload labels, for
/// capability detection.
async fn releases_and_workloads(
    client: &kube::Client,
    facts: &mut ClusterFacts,
) -> Result<(), ClusterError> {
    let c = || client.clone();
    let helm_label = ListParams::default().labels("owner=helm");
    let (helm_secrets, deployments, stateful_sets, daemon_sets) = futures_util::join!(
        async {
            match Api::<Secret>::all(c()).list_metadata(&helm_label).await {
                Ok(list) => Ok(Listed::Items(
                    list.items.into_iter().map(|o| o.metadata).collect(),
                )),
                Err(error) => refusal(error, "secrets"),
            }
        },
        list_metadata(Api::<Deployment>::all(c()), "deployments.apps"),
        list_metadata(Api::<StatefulSet>::all(c()), "statefulsets.apps"),
        list_metadata(Api::<DaemonSet>::all(c()), "daemonsets.apps"),
    );
    let mut metas = Vec::new();
    take(helm_secrets?, &mut metas, &mut facts.unreadable);
    // The `name` label is the release's name; the chart is only in the encoded record.
    facts.helm_releases = metas
        .into_iter()
        .filter_map(|m| {
            let name = m.labels?.remove("name")?;
            Some(format!("{}/{name}", m.namespace.unwrap_or_default()))
        })
        .collect();
    for (kind, listed) in [
        ("Deployment", deployments?),
        ("StatefulSet", stateful_sets?),
        ("DaemonSet", daemon_sets?),
    ] {
        let mut metas = Vec::new();
        take(listed, &mut metas, &mut facts.unreadable);
        facts.workload_labels.extend(
            metas
                .into_iter()
                .map(|m| (kind.to_owned(), m.labels.unwrap_or_default())),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How an exec ends when the kubelet sends this status.
    fn ended(json: serde_json::Value) -> ExitStatus {
        exit_status(Some(serde_json::from_value(json).unwrap()))
    }

    #[test]
    fn an_exec_ends_with_the_kubelets_exit_code() {
        assert_eq!(
            ended(serde_json::json!({"status": "Success"})),
            ExitStatus::Exited(0)
        );
        let failed = ended(serde_json::json!({
            "status": "Failure", "reason": "NonZeroExitCode",
            "message": "command terminated with non-zero exit code: error executing command [sh -c exit 3], exit code 3",
            "details": {"causes": [{"reason": "ExitCode", "message": "3"}]}
        }));
        assert_eq!(failed, ExitStatus::Exited(3));
    }

    #[test]
    fn a_missing_shell_is_told_apart_from_other_failures() {
        let missing = ended(serde_json::json!({
            "status": "Failure", "reason": "InternalError",
            "message": "Internal error occurred: error executing command in container: failed to exec in container: failed to start exec \"x\": OCI runtime exec failed: exec failed: unable to start container process: exec: \"/bin/sh\": stat /bin/sh: no such file or directory: unknown"
        }));
        assert_eq!(missing, ExitStatus::CommandNotFound);
        let other = ended(serde_json::json!({
            "status": "Failure", "reason": "InternalError", "message": "container is paused"
        }));
        assert_eq!(other, ExitStatus::Failed("container is paused".to_owned()));
        assert!(matches!(exit_status(None), ExitStatus::Failed(_)));
    }

    #[test]
    fn a_refused_exec_upgrade_keeps_its_code() {
        let error = kube::Error::UpgradeConnection(UpgradeConnectionError::ProtocolSwitch(
            http::StatusCode::FORBIDDEN,
        ));
        let ClusterError::Status(status) = ClusterError::from(error) else {
            panic!("not a status");
        };
        assert_eq!((status.code, status.reason.as_str()), (403, "Forbidden"));
    }

    #[test]
    fn the_proxy_path_names_the_service_port_and_stays_under_it() {
        let service = ServicePort {
            namespace: "observability".to_owned(),
            name: "vm".to_owned(),
            port: "8428".to_owned(),
        };
        assert_eq!(
            proxy_path(&service, "/api/v1/query_range?query=up&step=15").unwrap(),
            "/api/v1/namespaces/observability/services/vm:8428/proxy/api/v1/query_range?query=up&step=15"
        );
        for path in ["api/v1/query", "/../../secrets", "/api/../x?y=1"] {
            assert!(proxy_path(&service, path).is_err(), "{path}");
        }
    }

    #[test]
    fn kube_apiservers_retry_hint_is_kept() {
        let status: kube::core::Status = serde_json::from_value(serde_json::json!({
            "kind": "Status", "apiVersion": "v1", "status": "Failure", "code": 429,
            "reason": "TooManyRequests", "message": "storage is (re)initializing",
            "details": {"retryAfterSeconds": 1}
        }))
        .unwrap();
        let ClusterError::Status(kept) = ClusterError::from(kube::Error::Api(status.boxed()))
        else {
            panic!("not a status");
        };
        assert_eq!(kept.retry_after_seconds, Some(1));
        assert_eq!((kept.code, kept.reason.as_str()), (429, "TooManyRequests"));
    }

    /// Given kube-apiserver's two kinds of `422` to a JSON Patch (bodies as k3s v1.36.5 sends
    /// them), the one for a patch that does not apply names no field, the one for an invalid
    /// result does.
    #[test]
    fn a_patch_that_does_not_apply_is_told_from_an_invalid_one() {
        let kept = |body: serde_json::Value| {
            let status: kube::core::Status = serde_json::from_value(body).unwrap();
            let ClusterError::Status(kept) = ClusterError::from(kube::Error::Api(status.boxed()))
            else {
                panic!("not a status");
            };
            kept
        };
        let not_applied = kept(serde_json::json!({
            "kind": "Status", "apiVersion": "v1", "status": "Failure", "code": 422,
            "reason": "Invalid", "details": {},
            "message": "the server rejected our request due to an error in our request"
        }));
        assert!(!not_applied.names_invalid_field);
        assert_eq!(not_applied.details, None);
        let invalid = kept(serde_json::json!({
            "kind": "Status", "apiVersion": "v1", "status": "Failure", "code": 422,
            "reason": "Invalid",
            "message": "Deployment.apps \"d\" is invalid: spec.replicas: Invalid value: -1",
            "details": {"name": "d", "group": "apps", "kind": "Deployment", "causes": [
                {"reason": "FieldValueInvalid", "message": "Invalid value: -1",
                 "field": "spec.replicas"}]}
        }));
        assert!(invalid.names_invalid_field);
        // The object and the field travel on, for the problem's summary and `fields`.
        assert_eq!(
            invalid.details.as_deref(),
            Some(&StatusDetails {
                object: Some(("Deployment".to_owned(), "d".to_owned())),
                causes: vec![FieldCause {
                    field: "spec.replicas".to_owned(),
                    reason: "FieldValueInvalid".to_owned(),
                    message: "Invalid value: -1".to_owned(),
                }],
            })
        );
    }

    #[test]
    fn a_secret_table_gives_type_and_key_count_but_no_values() {
        let table = serde_json::json!({
            "kind": "Table",
            "apiVersion": "meta.k8s.io/v1",
            "columnDefinitions": [
                {"name": "Name"}, {"name": "Type"}, {"name": "Data"}, {"name": "Age"}
            ],
            "rows": [{
                "cells": ["db", "Opaque", 2, "5d"],
                "object": {"kind": "PartialObjectMetadata", "apiVersion": "meta.k8s.io/v1",
                           "metadata": {"name": "db", "namespace": "shop",
                                        "labels": {"app": "db"}}}
            }]
        });
        let secrets = parse_secret_table(&table).unwrap();
        assert_eq!(secrets.len(), 1);
        let summary = &secrets[0];
        assert_eq!(summary.secret.metadata.name.as_deref(), Some("db"));
        assert_eq!(summary.secret.type_.as_deref(), Some("Opaque"));
        assert_eq!(summary.key_count, 2);
        assert!(summary.secret.data.is_none() && summary.secret.string_data.is_none());
    }

    #[test]
    fn a_secret_table_hides_the_values_kubectl_apply_copied_into_an_annotation() {
        let applied = r#"{"apiVersion":"v1","data":{"password":"aHVudGVyMg=="},"kind":"Secret"}"#;
        let table = serde_json::json!({
            "kind": "Table",
            "apiVersion": "meta.k8s.io/v1",
            "columnDefinitions": [{"name": "Name"}, {"name": "Type"}, {"name": "Data"}],
            "rows": [{
                "cells": ["db", "Opaque", 1],
                "object": {"kind": "PartialObjectMetadata", "apiVersion": "meta.k8s.io/v1",
                           "metadata": {"name": "db", "namespace": "shop", "annotations": {
                               "kubectl.kubernetes.io/last-applied-configuration": applied
                           }}}
            }]
        });
        let secrets = parse_secret_table(&table).unwrap();
        let text = serde_json::to_string(&secrets[0].secret).unwrap();
        assert!(!text.contains("aHVudGVyMg=="), "{text}");
        assert_eq!(
            secrets[0].secret.metadata.annotations.as_ref().unwrap()["kubectl.kubernetes.io/last-applied-configuration"],
            objects::HIDDEN
        );
    }
}
