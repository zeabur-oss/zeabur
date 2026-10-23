//! Error pages and maintenance on a domain.
//!
//! - **The server.** Each project that uses them has one: a Deployment, a Service and a ConfigMap
//!   named [`SERVER`]. The first `errorPageHtml` change makes them, with all four pages. The
//!   ConfigMap holds `502.html`, `503.html`, `504.html`, `maintenance.html` and `nginx.conf`. nginx
//!   reads a page from the ConfigMap volume on each request, so a new page needs no restart; the
//!   kubelet updates the volume after its sync period [k8s-configmap]. Port [`PAGES_PORT`] serves
//!   each page by its file name. Port [`MAINTENANCE_PORT`] answers every request with
//!   `maintenance.html` and status 503. The pod passes the `restricted` Pod Security level.
//! - **Error pages on an Ingress** are a Traefik `errors` middleware [traefik-errors] for statuses
//!   502 to 504 that asks the pages port for `/{status}.html`. The Ingress lists it last and owns
//!   it. Traefik finds the Service in the middleware's namespace, so the server is in the project.
//!   Gateway API has no standard filter for error pages, so an `HTTPRoute` refuses them.
//! - **Maintenance** sends the hostname's requests to the maintenance port, and the workload keeps
//!   running. [`MAINTENANCE_ANNOTATION`] keeps the backends the hostname had, and stopping writes
//!   them back. On an Ingress these are the backends of the hostname's paths. On an `HTTPRoute`
//!   they are the `backendRefs` of its rules, and the route must serve only that hostname, as for a
//!   redirect. The `errors` middleware would replace the maintenance page with `503.html`, so while
//!   a hostname of the Ingress is in maintenance, the middleware catches only 502 and 504.
//!
//! [k8s-configmap]: https://kubernetes.io/docs/concepts/configuration/configmap/#mounted-configmaps-are-updated-automatically
//! [traefik-errors]: https://doc.traefik.io/traefik/reference/routing-configuration/http/middlewares/errorpages/

use std::collections::BTreeMap;

use k8s_openapi::api::apps::v1::Deployment;
use k8s_openapi::api::core::v1::{ConfigMap, Service};
use serde_json::{Value, json};
use zeabur_adapters::error_pages::{
    MAINTENANCE_ANNOTATION, MAINTENANCE_PORT, PAGES_PORT, SERVER, ingress_services, maintenance,
};
use zeabur_adapters::traefik;

use crate::access::{meta_of, middleware_resource, middleware_target, owner_of};
use crate::change::{Change, ErrorPageHtml, RouteKind, RouteRef, Target};
use crate::checks::ResourceRef;
use crate::fields::Step;
use crate::json::{add, pointer, replace, test};
use crate::plan::{
    ErrorKind, PlanError, Planner, array, error, http_route_target, ingress_target, to_json,
};

/// nginx-unprivileged 1.30.5 (stable) on Alpine, without modules, by the digest of its image
/// index. It runs as user 101 and listens on unprivileged ports.
pub(crate) const IMAGE: &str = "docker.io/nginxinc/nginx-unprivileged:1.30.5-alpine-slim@sha256:7f79f463c6eb0d0bf8782992bb9636434398e3e5848398805aef2c9b025d8609";

/// The largest page, so that the ConfigMap stays far below its limit of 1 MiB.
const PAGE_MAX: usize = 64 * 1024;

const NGINX_CONF: &str = r#"worker_processes 1;
pid /tmp/nginx.pid;
error_log /dev/stderr warn;
events {
    worker_connections 256;
}
http {
    include /etc/nginx/mime.types;
    default_type application/octet-stream;
    charset utf-8;
    server_tokens off;
    access_log off;
    client_body_temp_path /tmp/client_temp;
    proxy_temp_path /tmp/proxy_temp;
    fastcgi_temp_path /tmp/fastcgi_temp;
    uwsgi_temp_path /tmp/uwsgi_temp;
    scgi_temp_path /tmp/scgi_temp;
    root /usr/share/nginx/html;
    server {
        listen 8080;
    }
    server {
        listen 8081;
        add_header Cache-Control "no-store" always;
        error_page 404 503 =503 /maintenance.html;
        location / {
            return 503;
        }
        location = /maintenance.html {
            internal;
        }
    }
}
"#;

/// The Traefik `Middleware` a change reads first, if any: the plan updates or deletes it.
pub(crate) fn middleware_read(change: &Change) -> Option<(ResourceRef, String)> {
    let route = if let Change::ErrorPages(pages) = change {
        &pages.route
    } else if let Change::Maintenance(maintenance) = change {
        &maintenance.route
    } else {
        return None;
    };
    (route.kind == RouteKind::Ingress).then(|| {
        (
            middleware_resource(),
            traefik::error_pages_name(&route.name),
        )
    })
}

/// Each page given, by its ConfigMap key.
fn given(pages: &ErrorPageHtml) -> Result<Vec<(&'static str, &str)>, PlanError> {
    let given: Vec<(&'static str, &str)> = [
        ("502.html", &pages.bad_gateway),
        ("503.html", &pages.service_unavailable),
        ("504.html", &pages.gateway_timeout),
        ("maintenance.html", &pages.maintenance),
    ]
    .into_iter()
    .filter_map(|(key, html)| html.as_deref().map(|html| (key, html)))
    .collect();
    if given.is_empty() {
        return Err(error(
            ErrorKind::BadRequest,
            "no_pages",
            "Give the HTML of at least one page.",
        ));
    }
    if let Some((key, _)) = given.iter().find(|(_, html)| html.len() > PAGE_MAX) {
        return Err(error(
            ErrorKind::BadRequest,
            "page_too_large",
            format!("{key} is larger than 64 KiB."),
        ));
    }
    Ok(given)
}

/// The errors middleware's statuses: without 503 while a hostname is in maintenance.
fn statuses(in_maintenance: bool) -> Value {
    if in_maintenance {
        json!(["502", "504"])
    } else {
        json!(["502-504"])
    }
}

fn bad_annotation(name: &str) -> PlanError {
    error(
        ErrorKind::Invalid,
        "bad_maintenance_annotation",
        format!("The {MAINTENANCE_ANNOTATION} annotation of {name} is not a JSON object."),
    )
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "the planner's change kinds are split across modules by area"
)]
impl Planner<'_> {
    pub(crate) fn error_page_html(&mut self, pages: &ErrorPageHtml) -> Result<(), PlanError> {
        let given = given(pages)?;
        let target = Target::of::<ConfigMap>(SERVER);
        let existing = self.input.snapshot.config_map(SERVER).map(to_json);
        if let Some(object) = existing.transpose()? {
            let operations: Vec<Value> = given
                .iter()
                .filter(|(key, html)| {
                    object
                        .pointer(&pointer(&["data", key]))
                        .and_then(Value::as_str)
                        != Some(html)
                })
                .map(|(key, html)| add(&pointer(&["data", key]), json!(html)))
                .collect();
            if operations.is_empty() {
                return Err(error(
                    ErrorKind::BadRequest,
                    "same_pages",
                    "The pages already have this HTML.",
                ));
            }
            let index = self.working(target, object.clone());
            if object.get("data").is_none() {
                self.patch(index, vec![add("/data", json!({}))])?;
            }
            self.patch(index, operations)?;
        } else {
            if given.len() < 4 {
                return Err(error(
                    ErrorKind::BadRequest,
                    "pages_needed",
                    "The first error pages of a project need all four pages.",
                ));
            }
            let mut data: BTreeMap<&str, &str> = given.into_iter().collect();
            data.insert("nginx.conf", NGINX_CONF);
            let body = json!({
                "apiVersion": "v1",
                "kind": "ConfigMap",
                "metadata": self.server_metadata(),
                "data": data,
            });
            self.created(target, body);
        }
        self.ensure_server()
    }

    pub(crate) fn error_pages(&mut self, route: &RouteRef, enabled: bool) -> Result<(), PlanError> {
        if route.kind == RouteKind::HttpRoute {
            return Err(error(
                ErrorKind::Invalid,
                "error_pages_need_ingress",
                "Gateway API has no filter for error pages; add them to a domain on an Ingress.",
            ));
        }
        let object = self.checked_route(route, "")?;
        self.refuse_without_traefik(&object, &route.name)?;
        let ingress = route.name.as_str();
        let name = traefik::error_pages_name(ingress);
        let index = self.working(ingress_target(ingress), object);
        let meta = meta_of(&self.current(index));
        let listed = traefik::middlewares(&meta).contains(&name);
        let middleware = self.fetched_middleware(&name);
        if !enabled {
            if !listed && middleware.is_none() {
                return Err(error(
                    ErrorKind::NotFound,
                    "no_error_pages",
                    format!("Ingress {ingress} shows no error pages."),
                ));
            }
            if listed {
                self.unlist(index, &name)?;
            }
            if middleware.is_some() {
                self.delete(&middleware_resource(), &name, None)?;
            }
            return Ok(());
        }
        if listed && middleware.is_some() {
            return Err(error(
                ErrorKind::Conflict,
                "error_pages_shown",
                format!("Ingress {ingress} already shows the error pages."),
            ));
        }
        self.ensure_server()?;
        if middleware.is_none() {
            let busy = !maintenance(&meta)
                .ok_or_else(|| bad_annotation(ingress))?
                .is_empty();
            let spec = json!({"errors": {
                "status": statuses(busy),
                "service": {"name": SERVER, "port": PAGES_PORT},
                "query": "/{status}.html",
            }});
            self.created_middleware(&name, owner_of(&meta).as_ref(), &spec);
        }
        if !listed {
            self.list(index, &name, false)?;
        }
        Ok(())
    }

    pub(crate) fn maintenance(
        &mut self,
        route: &RouteRef,
        hostname: &str,
        enabled: bool,
    ) -> Result<(), PlanError> {
        let object = self.checked_route(route, hostname)?;
        let target = match route.kind {
            RouteKind::Ingress => ingress_target(&route.name),
            RouteKind::HttpRoute => http_route_target(&route.name),
        };
        let index = self.working(target, object);
        let current = self.current(index);
        let mut kept =
            maintenance(&meta_of(&current)).ok_or_else(|| bad_annotation(&route.name))?;
        match (enabled, kept.contains_key(hostname)) {
            (true, true) => {
                return Err(error(
                    ErrorKind::Conflict,
                    "in_maintenance",
                    format!("{hostname} already shows the maintenance page."),
                ));
            }
            (false, false) => {
                return Err(error(
                    ErrorKind::NotFound,
                    "not_in_maintenance",
                    format!("{hostname} does not show the maintenance page."),
                ));
            }
            _ => {}
        }
        if enabled {
            self.ensure_server()?;
        }
        self.claim(index, &[Step::field("spec"), Step::field("rules")], "rules")?;
        let rules = array(&current, "/spec/rules");
        let (operations, had) = match route.kind {
            RouteKind::Ingress => ingress_swap(&rules, hostname, kept.get(hostname))?,
            RouteKind::HttpRoute => {
                let count = array(&current, "/spec/hostnames").len();
                if count != 1 {
                    return Err(error(
                        ErrorKind::Invalid,
                        "shared_route",
                        format!(
                            "HTTPRoute {} serves {count} hostnames, and maintenance would apply \
                             to all of them. Move the hostname to a route of its own first.",
                            route.name
                        ),
                    ));
                }
                http_route_swap(&rules, kept.get(hostname))?
            }
        };
        self.patch(index, operations)?;
        if enabled {
            kept.insert(hostname.to_owned(), had);
        } else if let Some(back) = kept.remove(hostname) {
            let services = match route.kind {
                RouteKind::Ingress => ingress_services(&back),
                RouteKind::HttpRoute => elements(&back)
                    .iter()
                    .flat_map(elements)
                    .filter_map(|r| r.get("name").and_then(Value::as_str).map(str::to_owned))
                    .collect(),
            };
            let behind = self.behind(&services.into_iter().collect());
            self.impact.extend(behind);
        }
        let after = if kept.is_empty() {
            String::new()
        } else {
            serde_json::to_string(&kept)
                .map_err(|e| error(ErrorKind::Invalid, "encode", e.to_string()))?
        };
        self.set_annotation(index, MAINTENANCE_ANNOTATION, &after)?;
        if route.kind == RouteKind::Ingress {
            self.retune_errors(&route.name, !kept.is_empty())?;
        }
        Ok(())
    }

    /// Sets the statuses of the errors middleware of Ingress `ingress`, where it exists.
    fn retune_errors(&mut self, ingress: &str, busy: bool) -> Result<(), PlanError> {
        let name = traefik::error_pages_name(ingress);
        let target = middleware_target(&name);
        let index = match self.objects.iter().position(|w| w.target == target) {
            Some(index) if self.objects.get(index).is_some_and(|w| w.deleted) => return Ok(()),
            Some(index) => index,
            None => match self.fetched_middleware(&name) {
                Some(found) => self.working(target, found),
                None => return Ok(()),
            },
        };
        let path = "/spec/errors/status";
        let before = self.current(index).pointer(path).cloned();
        let after = statuses(busy);
        let operations = match before {
            Some(before) if before == after => return Ok(()),
            Some(before) => vec![test(path, before), replace(path, after)],
            None => vec![add(path, after)],
        };
        self.patch(index, operations)
    }

    /// Makes the server's Deployment and Service where they do not exist. The ConfigMap must
    /// exist, or come from an earlier change of this plan.
    fn ensure_server(&mut self) -> Result<(), PlanError> {
        let made = |planner: &Self, target: &Target| {
            planner
                .objects
                .iter()
                .any(|w| w.target == *target && !w.deleted)
        };
        let pages = Target::of::<ConfigMap>(SERVER);
        if self.input.snapshot.config_map(SERVER).is_none() && !made(self, &pages) {
            return Err(error(
                ErrorKind::NotFound,
                "no_error_page_html",
                "The project has no error pages yet; write them first with an errorPageHtml change.",
            ));
        }
        let snapshot = self.input.snapshot;
        let deployment = Target::of::<Deployment>(SERVER);
        let has_deployment = snapshot
            .deployments
            .iter()
            .any(|d| d.metadata.name.as_deref() == Some(SERVER));
        if !has_deployment && !made(self, &deployment) {
            let body = self.server_deployment();
            self.created(deployment, body);
        }
        let service = Target::of::<Service>(SERVER);
        let has_service = snapshot
            .services
            .iter()
            .any(|s| s.metadata.name.as_deref() == Some(SERVER));
        if !has_service && !made(self, &service) {
            let body = json!({
                "apiVersion": "v1",
                "kind": "Service",
                "metadata": self.server_metadata(),
                "spec": {
                    "selector": {"app.kubernetes.io/name": SERVER},
                    "ports": [
                        {"name": "pages", "port": PAGES_PORT, "targetPort": "pages",
                            "appProtocol": "http"},
                        {"name": "maintenance", "port": MAINTENANCE_PORT,
                            "targetPort": "maintenance", "appProtocol": "http"},
                    ],
                },
            });
            self.created(service, body);
        }
        Ok(())
    }

    fn server_metadata(&self) -> Value {
        json!({
            "name": SERVER,
            "namespace": self.input.snapshot.namespace,
            "labels": {"app.kubernetes.io/name": SERVER},
        })
    }

    fn server_deployment(&self) -> Value {
        let items: Vec<Value> = ["502.html", "503.html", "504.html", "maintenance.html"]
            .iter()
            .map(|key| json!({"key": key, "path": key}))
            .collect();
        json!({
            "apiVersion": "apps/v1",
            "kind": "Deployment",
            "metadata": self.server_metadata(),
            "spec": {
                "replicas": 1,
                "selector": {"matchLabels": {"app.kubernetes.io/name": SERVER}},
                "template": {
                    "metadata": {"labels": {"app.kubernetes.io/name": SERVER}},
                    "spec": {
                        "automountServiceAccountToken": false,
                        "securityContext": {
                            "runAsNonRoot": true,
                            "runAsUser": 101,
                            "runAsGroup": 101,
                            "seccompProfile": {"type": "RuntimeDefault"},
                        },
                        "containers": [{
                            "name": "nginx",
                            "image": IMAGE,
                            "command": ["nginx", "-g", "daemon off;"],
                            "ports": [
                                {"name": "pages", "containerPort": PAGES_PORT},
                                {"name": "maintenance", "containerPort": MAINTENANCE_PORT},
                            ],
                            "readinessProbe": {"tcpSocket": {"port": "pages"}},
                            "resources": {
                                "requests": {"cpu": "5m", "memory": "8Mi"},
                                "limits": {"memory": "32Mi"},
                            },
                            "securityContext": {
                                "allowPrivilegeEscalation": false,
                                "readOnlyRootFilesystem": true,
                                "capabilities": {"drop": ["ALL"]},
                            },
                            "volumeMounts": [
                                {"name": "pages", "mountPath": "/usr/share/nginx/html",
                                    "readOnly": true},
                                {"name": "config", "mountPath": "/etc/nginx/nginx.conf",
                                    "subPath": "nginx.conf", "readOnly": true},
                                {"name": "tmp", "mountPath": "/tmp"},
                            ],
                        }],
                        "volumes": [
                            {"name": "pages", "configMap": {"name": SERVER, "items": items}},
                            {"name": "config", "configMap": {"name": SERVER, "items": [
                                {"key": "nginx.conf", "path": "nginx.conf"},
                            ]}},
                            {"name": "tmp", "emptyDir": {"medium": "Memory", "sizeLimit": "8Mi"}},
                        ],
                    },
                },
            },
        })
    }
}

/// The patch that sends the paths of `hostname` to the maintenance port, or back to `kept`, and
/// the backends they had.
fn ingress_swap(
    rules: &[Value],
    hostname: &str,
    kept: Option<&Value>,
) -> Result<(Vec<Value>, Value), PlanError> {
    let page = json!({"service": {"name": SERVER, "port": {"number": MAINTENANCE_PORT}}});
    let back = kept.map(elements);
    let mut operations = Vec::new();
    let mut had = Vec::new();
    for (r, rule) in rules.iter().enumerate() {
        if rule.get("host").and_then(Value::as_str) != Some(hostname) {
            continue;
        }
        for (p, path) in array(rule, "/http/paths").iter().enumerate() {
            let Some(backend) = path.get("backend") else {
                continue;
            };
            let at = format!("/spec/rules/{r}/http/paths/{p}/backend");
            let next = match &back {
                Some(back) => back.get(had.len()).cloned().ok_or_else(changed)?,
                None => page.clone(),
            };
            operations.push(test(&at, backend.clone()));
            operations.push(replace(&at, next));
            had.push(backend.clone());
        }
    }
    finish_swap(operations, had, back.as_ref())
}

/// As [`ingress_swap`], for the `backendRefs` of each rule of an `HTTPRoute`.
fn http_route_swap(
    rules: &[Value],
    kept: Option<&Value>,
) -> Result<(Vec<Value>, Value), PlanError> {
    let page = json!([{"name": SERVER, "port": MAINTENANCE_PORT}]);
    let back = kept.map(elements);
    if back.as_ref().is_some_and(|b| b.len() != rules.len()) {
        return Err(changed());
    }
    let mut operations = Vec::new();
    let mut had = Vec::new();
    for (r, rule) in rules.iter().enumerate() {
        let refs = rule
            .get("backendRefs")
            .filter(|refs| !elements(refs).is_empty());
        let next = back.as_ref().map_or_else(
            || refs.map(|_| page.clone()),
            |back| back.get(r).filter(|b| !b.is_null()).cloned(),
        );
        had.push(refs.cloned().unwrap_or(Value::Null));
        let (Some(refs), Some(next)) = (refs, next) else {
            continue;
        };
        let at = format!("/spec/rules/{r}/backendRefs");
        operations.push(test(&at, refs.clone()));
        operations.push(replace(&at, next));
    }
    finish_swap(operations, had, back.as_ref())
}

fn finish_swap(
    operations: Vec<Value>,
    had: Vec<Value>,
    back: Option<&Vec<Value>>,
) -> Result<(Vec<Value>, Value), PlanError> {
    if operations.is_empty() {
        return Err(error(
            ErrorKind::Invalid,
            "no_backend",
            "The hostname sends its requests to no Service.",
        ));
    }
    if back.is_some_and(|b| b.len() != had.len()) {
        return Err(changed());
    }
    Ok((operations, Value::Array(had)))
}

fn elements(value: &Value) -> Vec<Value> {
    value.as_array().cloned().unwrap_or_default()
}

fn changed() -> PlanError {
    error(
        ErrorKind::Conflict,
        "maintenance_changed",
        format!(
            "The rules changed after maintenance started, so {MAINTENANCE_ANNOTATION} no longer \
             fits them. Restore the backends by hand."
        ),
    )
}
