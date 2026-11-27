//! Scheduled same-tag updates for CloudNativePG, running as the chart's updater ServiceAccount.

mod target;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use axum::Router;
use axum::http::StatusCode;
use axum::routing::get;
use futures_util::{StreamExt as _, stream};
use serde_json::json;
use zeabur_api::images::registry::{Transport, published};
use zeabur_kube::k8s_openapi::api::core::v1::{Node, Pod};
use zeabur_kube::k8s_openapi::jiff::Timestamp;
use zeabur_kube::{
    Cluster, ClusterEndpoint, ClusterError, ClusterExt as _, DynamicObject, ObjectTarget,
    UserClient, WriteBody, WriteRequest,
};
use zeabur_modules::cloudnative_pg::{CLUSTERS, ready_for_image_update};

use self::target::Target;

#[derive(Debug, clap::Parser)]
#[command(version)]
pub(crate) struct Args {
    /// Interval between reads of Cluster settings. Each database has its own update schedule.
    #[arg(long, default_value = "15s", value_parser = crate::components::duration)]
    scan_interval: Duration,
    /// Development registries (exact host[:port]) that serve plain HTTP.
    #[arg(long, value_delimiter = ',')]
    http_registries: Vec<String>,
    /// Extra trusted CA for registry connections.
    #[arg(long, env = "ZEABUR_EXTRA_CA_FILE")]
    extra_ca_file: Option<PathBuf>,
}

pub(crate) async fn run(args: &Args) -> Result<(), String> {
    if args.scan_interval.is_zero() {
        return Err("scan interval must be positive".to_owned());
    }
    let ca = crate::config::bundle(&[(&args.extra_ca_file, "--extra-ca-file")])?;
    let transport = zeabur_http::Https::new(
        zeabur_http::Reach::Public,
        &zeabur_http::tls(ca.as_deref())?,
    );
    let endpoint = ClusterEndpoint::in_cluster().map_err(|e| e.to_string())?;
    let cluster = UserClient::with_token_file(&endpoint, Path::new(crate::components::TOKEN_FILE))
        .map_err(|e| e.to_string())?;
    let ready = Arc::new(AtomicBool::new(false));
    let observed = Arc::clone(&ready);
    let probes = Router::new()
        .route("/healthz", get(|| async { StatusCode::OK }))
        .route(
            "/readyz",
            get(move || {
                let ready = observed.load(Ordering::Relaxed);
                async move {
                    if ready {
                        StatusCode::OK
                    } else {
                        StatusCode::SERVICE_UNAVAILABLE
                    }
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", 9090))
        .await
        .map_err(|e| e.to_string())?;
    let scan = async {
        let mut jobs = HashMap::new();
        let mut timer = tokio::time::interval(args.scan_interval);
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            timer.tick().await;
            match cluster.list_objects(&CLUSTERS.resource(), None).await {
                Ok(Some(objects)) => {
                    ready.store(true, Ordering::Relaxed);
                    let now = Timestamp::now();
                    let due = scheduled(&mut jobs, &objects, now);
                    stream::iter(due).for_each_concurrent(4, |(target, object)| {
                        let cluster = &cluster;
                        let transport = &transport;
                        async move {
                            let check = check(cluster, transport, &args.http_registries, &target, object);
                            let outcome = tokio::time::timeout(Duration::from_secs(45), check).await
                                .unwrap_or_else(|_| Err("update check timed out".to_owned()));
                            if let Err(error) = outcome {
                                tracing::warn!(namespace = target.namespace, database = target.name, %error, "database image check failed");
                            }
                        }
                    }).await;
                }
                Ok(None) => {
                    ready.store(false, Ordering::Relaxed);
                    jobs.clear();
                }
                Err(error) => {
                    ready.store(false, Ordering::Relaxed);
                    tracing::warn!(%error, "cannot list CloudNativePG Clusters");
                }
            }
        }
    };
    tokio::select! {
        () = scan => Ok(()),
        result = axum::serve(listener, probes).with_graceful_shutdown(crate::stop_signal()).into_future() => result.map_err(|e| e.to_string()),
    }
}

// Only the target's identity and inputs belong in the schedule key. CNPG updates its status
// frequently. Restarting or observing changed settings waits for the next scheduled check.
fn scheduled<'a>(
    jobs: &mut HashMap<String, (Target, Option<Timestamp>)>,
    objects: &'a [DynamicObject],
    now: Timestamp,
) -> Vec<(Target, &'a DynamicObject)> {
    let mut previous = std::mem::take(jobs);
    let mut due = Vec::new();
    for cluster in objects {
        let Some(target) = Target::read(cluster) else {
            continue;
        };
        let next = match previous.remove(&target.uid) {
            Some((old, next)) if old == target => {
                if next.is_some_and(|t| t <= now) {
                    due.push((target.clone(), cluster));
                    target.next(now)
                } else {
                    next
                }
            }
            _ => {
                let next = target.next(now);
                if next.is_none() {
                    tracing::warn!(
                        namespace = target.namespace,
                        database = target.name,
                        "database update schedule has no next occurrence"
                    );
                }
                next
            }
        };
        jobs.insert(target.uid.clone(), (target, next));
    }
    due
}

async fn check(
    cluster: &dyn Cluster,
    transport: &dyn Transport,
    http_registries: &[String],
    target: &Target,
    object: &DynamicObject,
) -> Result<(), String> {
    if !ready_for_image_update(object) {
        return Ok(());
    }
    let (tagged, pin) = target
        .image
        .split_once('@')
        .map_or((target.image.as_str(), None), |(t, p)| (t, Some(p)));
    let nodes = cluster
        .list::<Node>(None)
        .await
        .map_err(|e| e.to_string())?;
    let required = target.architectures(&nodes)?;
    let runtime_architectures = if pin.is_none() {
        required.iter().cloned().collect()
    } else {
        Vec::new()
    };
    let candidate = published(
        transport,
        cluster,
        &target.namespace,
        &target.pull_secrets,
        tagged,
        http_registries,
        &runtime_architectures,
    )
    .await?;
    if pin.is_some_and(|id| candidate.image_ids.contains(id)) {
        return Ok(());
    }
    if pin.is_none() {
        let pods = cluster
            .list::<Pod>(Some(&target.namespace))
            .await
            .map_err(|e| e.to_string())?;
        let instances = object
            .data
            .pointer("/status/readyInstances")
            .and_then(serde_json::Value::as_u64)
            .and_then(|n| usize::try_from(n).ok())
            .ok_or("unknown instance count")?;
        if target
            .running_ids(&pods, instances)?
            .iter()
            .all(|id| candidate.image_ids.contains(*id))
        {
            return Ok(());
        }
    }
    if !required.is_subset(&candidate.architectures) {
        return Err("published image does not support every eligible node architecture".to_owned());
    }
    apply(cluster, target, tagged, &candidate.digest).await
}

async fn apply(
    cluster: &dyn Cluster,
    target: &Target,
    tagged: &str,
    digest: &str,
) -> Result<(), String> {
    let resource = CLUSTERS.resource();
    let image = format!("{tagged}@{digest}");
    for _ in 0..3 {
        let current = cluster
            .find_object(&resource, Some(&target.namespace), &target.name)
            .await
            .map_err(|e| e.to_string())?;
        let Some(current) = current
            .filter(|c| ready_for_image_update(c) && Target::read(c).as_ref() == Some(target))
        else {
            return Ok(());
        };
        let version = current
            .metadata
            .resource_version
            .as_deref()
            .filter(|v| !v.is_empty())
            .ok_or("Cluster has no resource version")?;
        let request = WriteRequest {
            target: ObjectTarget {
                api_version: resource.api_version(),
                kind: resource.kind.clone(),
                plural: resource.resource.clone(),
                namespace: target.namespace.clone(),
                name: target.name.clone(),
            },
            dry_run: false,
            body: WriteBody::MergePatch(json!({
                "metadata": {"uid": target.uid, "resourceVersion": version, "annotations": {
                    "keel.sh/update-time": Timestamp::now().to_string(), "keel.sh/digest": digest,
                    "kubernetes.io/change-cause": format!("Zeabur database updater: {image}")
                }},
                "spec": {"imageName": image}
            })),
        };
        match cluster.write(&request).await {
            Ok(_) => {
                tracing::info!(namespace = target.namespace, database = target.name, %image, "database image updated");
                return Ok(());
            }
            Err(ClusterError::Status(status)) if status.code == 409 => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Err("Cluster changed during each update attempt".to_owned())
}

#[cfg(test)]
mod tests;
