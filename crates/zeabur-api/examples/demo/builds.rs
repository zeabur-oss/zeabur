//! The demo's Zeabur Builder builds, their runs, and the pods that ran them with their logs.
//!
//! Blog's docs, deployed from Git with Railpack: its first build has finished by the time the
//! page opens, and its pod is gone. `ZEABUR_DEMO_BUILD_RUNNING=1` keeps it building, with its pod
//! in the middle of `npm run build`; `ZEABUR_DEMO_BUILD_FAILED=1` makes its plan fail, and
//! `ZEABUR_DEMO_BUILD_OOM=1` makes it run out of memory.
//!
//! Lab's storefront, built from its Dockerfile, whose image declares a port the service has no
//! address for yet: its newest run succeeded a few minutes ago, most steps cached. With
//! `ZEABUR_DEMO_BUILD_FAILED=1` a newer run failed at `RUN npm ci`, and the image from before
//! keeps running.
//!
//! The build container writes the builder's NDJSON events, one per line; `fetch` and `plan` write
//! plain text.

use serde_json::{Value, json};
use zeabur_kube::DynamicObject;
use zeabur_kube::k8s_openapi::api::batch::v1::Job;
use zeabur_kube::k8s_openapi::api::core::v1::Pod;
use zeabur_kube::k8s_openapi::jiff::SignedDuration;
use zeabur_kube::memory::State;

use super::{clock, from};

/// Whether blog's docs stays building.
pub(super) fn docs_running() -> bool {
    std::env::var_os("ZEABUR_DEMO_BUILD_RUNNING").is_some()
}

fn failed() -> bool {
    std::env::var_os("ZEABUR_DEMO_BUILD_FAILED").is_some()
}

fn oom() -> bool {
    std::env::var_os("ZEABUR_DEMO_BUILD_OOM").is_some()
}

/// `seconds` before the demo's now.
fn ago(seconds: i64) -> String {
    let now = clock();
    now.checked_sub(SignedDuration::from_secs(seconds))
        .unwrap_or(now)
        .to_string()
}

/// The `Build`s and `BuildRun`s.
pub(super) fn objects() -> Vec<DynamicObject> {
    let mut out = docs();
    out.extend(storefront());
    out
}

fn docs() -> Vec<DynamicObject> {
    let failed = oom() || failed();
    let build = json!({
        "apiVersion": "builder.zeabur.com/v1alpha1", "kind": "Build",
        "metadata": {"name": "docs", "namespace": "blog"},
        "spec": {"builderRef": "railpack",
            "source": {"git": {"url": "https://github.com/example/docs.git", "ref": "main",
                "contextDir": "site"}},
            "output": {"image": "zot.zot.svc:5000/blog/docs"},
            "env": [{"name": "NPM_TOKEN", "valueFrom": {"secretKeyRef":
                {"name": "docs-build-env", "key": "NPM_TOKEN"}}}],
            "notify": [{"type": "keel",
                "url": "http://keel.keel.svc.cluster.local:9300/v1/webhooks/native"}]},
        "status": if failed && !oom() {
            json!({"conditions": [{"type": "Ready", "status": "False", "reason": "BuildFailed",
                "message": "railpack: no start command found"}]})
        } else {
            json!({})
        }
    });
    let status = if oom() {
        json!({"phase": "Failed", "reason": "OutOfMemory",
            "message": "buildkitd was killed for its memory. Set the annotation builder.zeabur.com/build-memory on the namespace to raise the limit."})
    } else if failed {
        json!({"phase": "Failed", "reason": "PlanFailed",
            "message": "railpack: no start command found. Add a start script to package.json."})
    } else if docs_running() {
        json!({"phase": "Building", "startTime": ago(70)})
    } else {
        json!({"phase": "Building", "startTime": "2026-10-23T08:00:05Z"})
    };
    let run = json!({
        "apiVersion": "builder.zeabur.com/v1alpha1", "kind": "BuildRun",
        "metadata": {"name": "docs-x7k2p", "namespace": "blog",
            "creationTimestamp": if docs_running() { ago(72) } else { "2026-10-23T08:00:00Z".to_owned() }},
        "spec": {"buildRef": "docs", "revision": "3f9a1c2b7d4e5f60718293a4b5c6d7e8f9012345",
            "trigger": "poll"},
        "status": status
    });
    vec![from(build), from(run)]
}

fn storefront() -> Vec<DynamicObject> {
    let ready = if failed() {
        json!({"type": "Ready", "status": "False", "reason": "BuildFailed",
            "message": "[4/5] RUN npm ci: process \"/bin/sh -c npm ci\" did not complete successfully: exit code: 1"})
    } else {
        json!({"type": "Ready", "status": "True", "reason": "Succeeded"})
    };
    let build = json!({
        "apiVersion": "builder.zeabur.com/v1alpha1", "kind": "Build",
        "metadata": {"name": "storefront", "namespace": "lab"},
        "spec": {"builderRef": "dockerfile",
            "source": {"git": {"url": "https://github.com/example/storefront.git", "ref": "main"}},
            "output": {"image": "zot.zot.svc:5000/lab/storefront"}},
        "status": {"latestImage": "zot.zot.svc:5000/lab/storefront:main-12-c81e4a0",
            "exposedPorts": ["3000/tcp"],
            "conditions": [{"type": "SourceReady", "status": "True", "reason": "Git"}, ready]}
    });
    let succeeded = json!({
        "apiVersion": "builder.zeabur.com/v1alpha1", "kind": "BuildRun",
        "metadata": {"name": "storefront-c81e4", "namespace": "lab",
            "creationTimestamp": ago(if failed() { 3 * 3600 } else { 300 })},
        "spec": {"buildRef": "storefront", "revision": "c81e4a0d9b2f3e4a5b6c7d8e9f0a1b2c3d4e5f60",
            "trigger": "webhook"},
        "status": {"phase": "Succeeded",
            "startTime": ago(if failed() { 3 * 3600 - 2 } else { 298 }),
            "completionTime": ago(if failed() { 3 * 3600 - 40 } else { 260 }),
            "tags": ["main-12-c81e4a0"]}
    });
    let mut out = vec![from(build), from(succeeded)];
    if failed() {
        out.push(from(json!({
            "apiVersion": "builder.zeabur.com/v1alpha1", "kind": "BuildRun",
            "metadata": {"name": "storefront-9d2f1", "namespace": "lab",
                "creationTimestamp": ago(300)},
            "spec": {"buildRef": "storefront",
                "revision": "9d2f1b7c4a5e6f708192a3b4c5d6e7f8091a2b3c", "trigger": "webhook"},
            "status": {"phase": "Failed", "reason": "BuildFailed",
                "startTime": ago(298), "completionTime": ago(270),
                "message": "[4/5] RUN npm ci: process \"/bin/sh -c npm ci\" did not complete successfully: exit code: 1"}
        })));
    }
    out
}

/// How a step of the build ends.
enum End {
    Running,
    Done(f64),
    Cached,
    Failed(f64, &'static str),
}

struct Step {
    name: &'static str,
    /// Seconds before now it started.
    start: i64,
    end: End,
    logs: &'static [&'static str],
}

/// The build container's log: each step's events, numbered in order.
fn events(steps: &[Step], messages: &[&str]) -> String {
    let mut out = Vec::new();
    let mut number = 0;
    for step in steps {
        number += 1;
        let base = json!({"vertex": number, "name": step.name});
        let event = |fields: Value| {
            let mut record = base.clone();
            if let (Some(record), Some(fields)) = (record.as_object_mut(), fields.as_object()) {
                record.extend(fields.clone());
            }
            record.to_string()
        };
        out.push(event(json!({"event": "started", "time": ago(step.start)})));
        for line in step.logs {
            out.push(event(
                json!({"event": "log", "stream": "stdout", "text": line}),
            ));
        }
        match step.end {
            End::Running => {}
            End::Done(seconds) => out.push(event(json!({"event": "done", "seconds": seconds}))),
            End::Cached => out.push(event(json!({"event": "cached"}))),
            End::Failed(seconds, message) => out.push(event(
                json!({"event": "error", "seconds": seconds, "message": message}),
            )),
        }
    }
    for text in messages {
        out.push(json!({"event": "message", "text": text}).to_string());
    }
    out.join("\n")
}

const NPM_CI_ERROR: &str =
    "process \"/bin/sh -c npm ci\" did not complete successfully: exit code: 1";

fn storefront_steps(failed: bool) -> Vec<Step> {
    let npm_ci = if failed {
        Step {
            name: "[4/5] RUN npm ci",
            start: 292,
            end: End::Failed(12.4, NPM_CI_ERROR),
            logs: &[
                "npm error code ERESOLVE",
                "npm error ERESOLVE could not resolve",
                "npm error While resolving: storefront@1.4.0",
                "npm error Found: react@19.2.0",
                "npm error Could not resolve dependency:",
                "npm error peer react@\"^18.0.0\" from react-helmet-async@2.0.5",
                "npm error Fix the upstream dependency conflict, or retry this command with --legacy-peer-deps.",
            ],
        }
    } else {
        Step {
            name: "[4/5] RUN npm ci",
            start: 292,
            end: End::Done(18.4),
            logs: &[
                "added 312 packages, and audited 313 packages in 17s",
                "found 0 vulnerabilities",
            ],
        }
    };
    let mut steps = vec![
        Step {
            name: "[internal] load build definition from Dockerfile",
            start: 296,
            end: End::Done(0.1),
            logs: &["transferring dockerfile: 312B done"],
        },
        Step {
            name: "[internal] load metadata for docker.io/library/node:22-alpine",
            start: 296,
            end: End::Done(1.2),
            logs: &[],
        },
        Step {
            name: "[internal] load .dockerignore",
            start: 295,
            end: End::Done(0.1),
            logs: &["transferring context: 64B done"],
        },
        Step {
            name: "[1/5] FROM docker.io/library/node:22-alpine",
            start: 294,
            end: End::Cached,
            logs: &[],
        },
        Step {
            name: "[internal] load build context",
            start: 294,
            end: End::Done(0.4),
            logs: &["transferring context: 1.21MB 0.3s done"],
        },
        Step {
            name: "[2/5] WORKDIR /app",
            start: 293,
            end: End::Cached,
            logs: &[],
        },
        Step {
            name: "[3/5] COPY package.json package-lock.json ./",
            start: 293,
            end: End::Cached,
            logs: &[],
        },
        npm_ci,
    ];
    if !failed {
        steps.push(Step {
            name: "[5/5] COPY . .",
            start: 273,
            end: End::Done(0.5),
            logs: &[],
        });
        steps.push(Step {
            name: "exporting to image",
            start: 272,
            end: End::Done(9.8),
            logs: &[
                "exporting layers 1.9s done",
                "exporting manifest sha256:5e1f0a3c 0.0s done",
                "pushing layers 7.4s done",
            ],
        });
    }
    steps
}

fn docs_steps() -> Vec<Step> {
    vec![
        Step {
            name: "[internal] load build definition from railpack-plan.json",
            start: 56,
            end: End::Done(0.1),
            logs: &[],
        },
        Step {
            name: "[internal] load metadata for ghcr.io/railwayapp/railpack-builder:latest",
            start: 56,
            end: End::Done(0.9),
            logs: &[],
        },
        Step {
            name: "install mise packages: node",
            start: 55,
            end: End::Done(6.2),
            logs: &["mise node@22.11.0 installed"],
        },
        Step {
            name: "copy package.json, package-lock.json",
            start: 48,
            end: End::Cached,
            logs: &[],
        },
        Step {
            name: "npm ci",
            start: 48,
            end: End::Done(21.3),
            logs: &[
                "added 512 packages, and audited 513 packages in 20s",
                "142 packages are looking for funding",
                "found 0 vulnerabilities",
            ],
        },
        Step {
            name: "npm run build",
            start: 26,
            end: End::Running,
            logs: &[
                "> docs@1.0.0 build",
                "> astro build",
                "generating static routes",
                "building client (vite)",
                "transforming...",
                "\u{2713} 412 modules transformed.",
                "rendering chunks...",
            ],
        },
    ]
}

/// A container's state in a pod's status.
fn container(name: &str, state: &Value) -> Value {
    json!({"name": name, "image": "example", "imageID": "", "ready": false, "restartCount": 0,
        "state": state})
}

fn terminated(start: i64, end: i64, code: i32) -> Value {
    json!({"terminated": {"exitCode": code, "reason": if code == 0 { "Completed" } else { "Error" },
        "startedAt": ago(start), "finishedAt": ago(end)}})
}

/// The Job's pod for a run: its init containers and `build`, in that order.
fn pod(namespace: &str, run: &str, name: &str, init: &[(&str, Value)], build: &Value) -> Pod {
    let specs: Vec<Value> = init
        .iter()
        .map(|(c, _)| json!({"name": c, "image": "example"}))
        .collect();
    let statuses: Vec<Value> = init.iter().map(|(c, s)| container(c, s)).collect();
    from(json!({
        "metadata": {"name": name, "namespace": namespace,
            "creationTimestamp": ago(300),
            "labels": {"builder.zeabur.com/buildrun": run, "batch.kubernetes.io/job-name": run},
            "ownerReferences": [{"apiVersion": "batch/v1", "kind": "Job", "name": run,
                "uid": format!("job-{run}"), "controller": true}]},
        "spec": {"nodeName": "vps-2", "restartPolicy": "Never",
            "initContainers": specs,
            "containers": [{"name": "build", "image": "example"}]},
        "status": {"phase": if build.get("running").is_some() { "Running" } else { "Succeeded" },
            "initContainerStatuses": statuses,
            "containerStatuses": [container("build", build)]}
    }))
}

/// The pods that ran the builds, and what each container logged.
pub(super) fn pods(s: &mut State) {
    let mut pods = Vec::new();
    let failed = failed();
    let run = if failed {
        "storefront-9d2f1"
    } else {
        "storefront-c81e4"
    };
    let name = format!("{run}-h2v6q");
    let build = terminated(292, if failed { 270 } else { 262 }, i32::from(failed));
    pods.push((
        "lab",
        pod(
            "lab",
            run,
            &name,
            &[("fetch", terminated(298, 296, 0))],
            &build,
        ),
    ));
    let messages: &[&str] = if failed {
        &[
            "error: failed to solve: process \"/bin/sh -c npm ci\" did not complete successfully: exit code: 1",
        ]
    } else {
        &[]
    };
    s.logs.insert(
        format!("lab/{name}/fetch"),
        "Cloning https://github.com/example/storefront.git (main)\nHEAD is now at 9d2f1b7 Bump react to 19"
            .to_owned(),
    );
    s.logs.insert(
        format!("lab/{name}/build"),
        events(&storefront_steps(failed), messages),
    );
    if docs_running() {
        let name = "docs-x7k2p-m4c8r";
        let init = [
            ("fetch", terminated(68, 64, 0)),
            ("plan", terminated(64, 57, 0)),
        ];
        let build = json!({"running": {"startedAt": ago(57)}});
        pods.push(("blog", pod("blog", "docs-x7k2p", name, &init, &build)));
        s.logs.insert(
            format!("blog/{name}/fetch"),
            "Cloning https://github.com/example/docs.git (main)\nHEAD is now at 3f9a1c2 Add the deploy guide"
                .to_owned(),
        );
        s.logs.insert(
            format!("blog/{name}/plan"),
            "Detected Node\nUsing npm\nInstall: npm ci\nBuild: npm run build\nStart: npx serve dist"
                .to_owned(),
        );
        s.logs
            .insert(format!("blog/{name}/build"), events(&docs_steps(), &[]));
    }
    for (namespace, pod) in pods {
        let Some(objects) = s.objects.get_mut(namespace) else {
            continue;
        };
        let run = pod
            .metadata
            .labels
            .as_ref()
            .and_then(|l| l.get("builder.zeabur.com/buildrun"))
            .cloned()
            .unwrap_or_default();
        objects.jobs.push(from::<Job>(json!({
            "metadata": {"name": run, "namespace": namespace, "uid": format!("job-{run}"),
                "creationTimestamp": ago(300),
                "ownerReferences": [{"apiVersion": "builder.zeabur.com/v1alpha1",
                    "kind": "BuildRun", "name": run, "uid": format!("run-{run}"),
                    "controller": true}]},
            "spec": {"template": {"spec": {"containers": [{"name": "build", "image": "example"}]}}}
        })));
        objects.pods.push(pod);
    }
}
