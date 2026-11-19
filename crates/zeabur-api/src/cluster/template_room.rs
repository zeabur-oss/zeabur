//! Whether the cluster has room for a template.
//!
//! Each of its pods needs the larger of what it used once idle ([`zeabur_catalog::measured`]) and
//! what its service requests. Each must fit on one ready, schedulable node that keeps a tenth of
//! its allocatable free, as restore copy places pods (`capacity::place`). Host use outside pods
//! is not subtracted, so "fits" means the pods may fit.

use std::collections::BTreeMap;

use axum::extract::State;
use serde::Serialize;
use utoipa::ToSchema;
use zeabur_catalog::IdleUse;
use zeabur_catalog::templates::TemplateService;
use zeabur_core::problem::{MEDIA_TYPE as PROBLEM, Problem};

use crate::cluster::capacity::{self, Amount, Fit, NodeRoom, place};
use crate::cluster::templates::render_problem;
use crate::error::ApiError;
use crate::extract::Json;
use crate::handlers::{AppState, Person};

/// Whether the cluster has room for a template's pods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TemplateRoom {
    /// Whether each pod fits on a node.
    pub fit: Fit,
    /// The resource `need` and `free` are about: the one that is short, else memory.
    pub resource: RoomResource,
    /// What the template's pods need of it in all: bytes or millicores.
    pub need: u64,
    /// What the ready, schedulable nodes have free of it in all.
    pub free: u64,
}

/// CPU or memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum RoomResource {
    /// Bytes.
    Memory,
    /// Millicores.
    Cpu,
}

/// The room for each template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct TemplateRooms {
    /// By template id.
    pub items: BTreeMap<String, TemplateRoom>,
}

/// Whether the cluster has room for each template's pods, read as the person once for all of
/// them: `unknown` when the nodes or the pods cannot be read whole.
#[utoipa::path(get, path = "/api/v1/cluster/template-rooms", tag = "cluster",
    operation_id = "listTemplateRooms",
    responses((status = 200, body = TemplateRooms), (status = 500, body = Problem, content_type = PROBLEM)))]
pub(crate) async fn rooms(
    State(state): State<AppState>,
    Person(cluster): Person,
) -> Result<Json<TemplateRooms>, ApiError> {
    let templates = state
        .templates
        .current()
        .templates()
        .map_err(|e| render_problem(&e))?;
    let reading = capacity::read(cluster.as_ref()).await;
    let items = templates
        .iter()
        .map(|t| {
            let pods = pods(&t.services, (state.template_idle)(&t.id).as_ref());
            (t.id.clone(), verdict(&pods, reading.nodes.as_deref()))
        })
        .collect();
    Ok(Json(TemplateRooms { items }))
}

/// One pod per service or measured workload: the larger of its requests and its idle use.
fn pods(services: &[TemplateService], idle: Option<&IdleUse>) -> Vec<Amount> {
    let mut pods: BTreeMap<&str, Amount> = services
        .iter()
        .map(|s| (s.name.as_str(), Amount::of(&s.requests)))
        .collect();
    for pod in idle.map_or(&[][..], |i| i.pods.as_slice()) {
        let measured = Amount {
            cpu: pod.cpu_millicores,
            memory: pod.memory_bytes,
        };
        let entry = pods.entry(pod.workload.as_str()).or_default();
        *entry = entry.max(measured);
    }
    pods.into_values().collect()
}

fn verdict(pods: &[Amount], nodes: Option<&[NodeRoom]>) -> TemplateRoom {
    let need: Amount = pods.iter().copied().sum();
    let free: Amount = nodes.unwrap_or_default().iter().map(|n| n.free).sum();
    let report = |fit, resource| {
        let (need, free) = match resource {
            RoomResource::Memory => (need.memory, free.memory),
            RoomResource::Cpu => (need.cpu, free.cpu),
        };
        TemplateRoom {
            fit,
            resource,
            need,
            free,
        }
    };
    let Some(nodes) = nodes else {
        return report(Fit::Unknown, RoomResource::Memory);
    };
    let kept: Vec<NodeRoom> = nodes.iter().map(|n| n.less_a_tenth()).collect();
    if place(pods, &kept) {
        return report(Fit::Fits, RoomResource::Memory);
    }
    let fits_by = |keep: fn(Amount) -> Amount| {
        let pods: Vec<Amount> = pods.iter().copied().map(keep).collect();
        place(&pods, &kept)
    };
    let memory_only = |a: Amount| Amount { cpu: 0, ..a };
    let cpu_only = |a: Amount| Amount { memory: 0, ..a };
    let resource = if fits_by(memory_only) && !fits_by(cpu_only) {
        RoomResource::Cpu
    } else {
        RoomResource::Memory
    };
    report(Fit::Short, resource)
}

#[cfg(test)]
mod tests {
    use zeabur_catalog::MeasuredPod;

    use super::*;

    const GIB: u64 = 1 << 30;

    fn node(cpu: u64, memory: u64) -> NodeRoom {
        NodeRoom {
            allocatable: Amount { cpu, memory },
            free: Amount { cpu, memory },
            pods: 110,
        }
    }

    fn service(name: &str, cpu: &str, memory: &str) -> TemplateService {
        TemplateService {
            name: name.to_owned(),
            requests: [("cpu", cpu), ("memory", memory)]
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                .collect(),
        }
    }

    fn idle(pods: &[(&str, u64, u64)]) -> IdleUse {
        IdleUse {
            memory_bytes: pods.iter().map(|p| p.2).sum(),
            cpu_millicores: pods.iter().map(|p| p.1).sum(),
            environment: "kind-single-node-amd64".to_owned(),
            measured_at: "2026-11-01".to_owned(),
            pods: pods
                .iter()
                .map(|&(workload, cpu_millicores, memory_bytes)| MeasuredPod {
                    workload: workload.to_owned(),
                    memory_bytes,
                    cpu_millicores,
                })
                .collect(),
        }
    }

    #[test]
    fn each_pod_needs_the_larger_of_its_requests_and_its_idle_use() {
        let services = [
            service("app", "500m", "64Mi"),
            service("app-db", "100m", "256Mi"),
        ];
        let measured = idle(&[
            ("app", 3, GIB),
            ("app-db", 1, 10 << 20),
            ("cache", 2, 1 << 20),
        ]);
        let mut pods = pods(&services, Some(&measured));
        pods.sort_by_key(|p| p.memory);
        assert_eq!(
            pods,
            [
                Amount {
                    cpu: 2,
                    memory: 1 << 20
                },
                Amount {
                    cpu: 100,
                    memory: 256 << 20
                },
                Amount {
                    cpu: 500,
                    memory: GIB
                },
            ]
        );
    }

    #[test]
    fn a_pod_fits_only_on_one_node_not_across_two() {
        let nodes = [node(4000, GIB + GIB / 5), node(4000, GIB + GIB / 5)];
        let room = verdict(
            &[Amount {
                cpu: 100,
                memory: 2 * GIB,
            }],
            Some(&nodes),
        );
        assert_eq!(room.fit, Fit::Short);
        assert_eq!(room.resource, RoomResource::Memory);
        assert_eq!(
            verdict(
                &[Amount {
                    cpu: 100,
                    memory: GIB
                }],
                Some(&nodes)
            )
            .fit,
            Fit::Fits
        );
    }

    #[test]
    fn a_tenth_of_each_node_stays_free() {
        let room = verdict(
            &[Amount {
                cpu: 100,
                memory: GIB,
            }],
            Some(&[node(4000, GIB)]),
        );
        assert_eq!(room.fit, Fit::Short);
    }

    #[test]
    fn the_short_resource_is_named() {
        let room = verdict(
            &[Amount {
                cpu: 3000,
                memory: GIB,
            }],
            Some(&[node(2000, 8 * GIB)]),
        );
        assert_eq!(
            (room.fit, room.resource, room.need, room.free),
            (Fit::Short, RoomResource::Cpu, 3000, 2000)
        );
    }

    #[test]
    fn without_the_nodes_the_room_is_unknown() {
        assert_eq!(verdict(&[Amount::default()], None).fit, Fit::Unknown);
    }
}
