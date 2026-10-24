//! Error pages and maintenance on a domain.
//!
//! - **The page server.** The person deploys it in the project from Zeabur Catalog's `error-pages`
//!   template, or with a `pageServer` change that the API opens into the `create` of each of its
//!   objects. Zeabur finds its Service and its ConfigMap of pages by [`PAGE_SERVER_LABEL`], and
//!   never makes or owns them. Port [`PAGES_PORT`] serves each page by its file name. Port
//!   [`MAINTENANCE_PORT`] answers every request with `maintenance.html` and status 503. nginx reads
//!   a page from the ConfigMap volume on each request, so a new page needs no restart; the kubelet
//!   updates the volume after its sync period [k8s-configmap].
//! - **Error pages on an Ingress** are a Traefik `errors` middleware [traefik-errors] for statuses
//!   502 to 504 that asks the pages port for `/{status}.html`. The Ingress lists it last and owns
//!   it. Traefik finds the Service in the middleware's namespace, so the page server is in the
//!   project. Gateway API has no standard filter for error pages, so an `HTTPRoute` refuses them.
//! - **Maintenance** sends the hostname's requests to the maintenance port, and the workload keeps
//!   running. [`MAINTENANCE_ANNOTATION`] keeps the backends the hostname had, and stopping writes
//!   them back. On an Ingress these are the backends of the hostname's paths. On an `HTTPRoute`
//!   they are the `backendRefs` of its rules. A filter applies to every hostname of the route, so a
//!   hostname that shares one moves to a route of its own first, as for a redirect. When the rules
//!   changed since, the kept backends no longer fit, and stopping sends the hostname to the
//!   workload's first Service instead.
//! - The `errors` middleware catches the maintenance port's 503 too, and asks for `503.html` with
//!   the original `Host` header [traefik-errors-source]. Its query lists the hostnames in
//!   maintenance (`?down=a.example.com,*.example.org`), and for those hosts the pages port answers
//!   `503.html` with `maintenance.html`. The other hostnames of the Ingress keep their 503 page.
//!
//! [k8s-configmap]: https://kubernetes.io/docs/concepts/configuration/configmap/#mounted-configmaps-are-updated-automatically
//! [traefik-errors]: https://doc.traefik.io/traefik/reference/routing-configuration/http/middlewares/errorpages/
//! [traefik-errors-source]: https://github.com/traefik/traefik/blob/v3.5.0/pkg/middlewares/customerrors/custom_errors.go

use k8s_openapi::api::core::v1::{ConfigMap, Service};
use serde_json::{Value, json};
use zeabur_adapters::error_pages::{
    MAINTENANCE_ANNOTATION, MAINTENANCE_PORT, PAGE_SERVER_LABEL, PAGES_PORT, PageServer,
    ingress_services, is_page_server, maintenance, page_server,
};
use zeabur_adapters::traefik;
use zeabur_model::WorkloadRef;

use crate::access::{meta_of, middleware_resource, middleware_target, owner_of};
use crate::change::{Change, ErrorPageHtml, RouteKind, RouteRef, Target, Warning};
use crate::checks::ResourceRef;
use crate::fields::Step;
use crate::json::{add, pointer, replace, test};
use crate::plan::{
    ErrorKind, PlanError, Planner, array, error, http_route_target, ingress_target, to_json,
};

/// The largest page, so that the ConfigMap stays far below its limit of 1 MiB.
const PAGE_MAX: usize = 64 * 1024;

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

/// The errors middleware's query, naming the hostnames in maintenance.
fn query<'h>(down: impl IntoIterator<Item = &'h String>) -> String {
    let down: Vec<&str> = down.into_iter().map(String::as_str).collect();
    if down.is_empty() {
        "/{status}.html".to_owned()
    } else {
        format!("/{{status}}.html?down={}", down.join(","))
    }
}

/// The `errors` middleware spec of an Ingress whose hostnames `down` are in maintenance.
pub(crate) fn errors_spec<'h>(
    server: &PageServer,
    down: impl IntoIterator<Item = &'h String>,
) -> Value {
    json!({"errors": {
        "status": ["502-504"],
        "service": {"name": server.name, "port": server.pages},
        "query": query(down),
    }})
}

fn no_page_server() -> PlanError {
    error(
        ErrorKind::NotFound,
        "no_page_server",
        format!(
            "The project has no page server: deploy the error-pages template, or another Service \
             labelled {PAGE_SERVER_LABEL}=true with the ports {PAGES_PORT} and {MAINTENANCE_PORT}."
        ),
    )
}

fn several_page_servers(kind: &str, names: &[String]) -> PlanError {
    error(
        ErrorKind::Conflict,
        "several_page_servers",
        format!(
            "Several {kind}s are labelled {PAGE_SERVER_LABEL}=true ({}); keep the label on one.",
            names.join(", ")
        ),
    )
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
        let labelled: Vec<&ConfigMap> = self
            .input
            .snapshot
            .config_maps
            .iter()
            .filter(|c| is_page_server(&c.metadata))
            .collect();
        let found = match labelled.as_slice() {
            [] => return Err(no_page_server()),
            [one] => *one,
            many => {
                let names: Vec<String> = many
                    .iter()
                    .filter_map(|c| c.metadata.name.clone())
                    .collect();
                return Err(several_page_servers("ConfigMap", &names));
            }
        };
        let name = found.metadata.name.clone().unwrap_or_default();
        Self::refuse_managed(&found.metadata, "ConfigMap", &name)?;
        let object = to_json(found)?;
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
        let index = self.working(Target::of::<ConfigMap>(&name), object.clone());
        if object.get("data").is_none() {
            self.patch(index, vec![add("/data", json!({}))])?;
        }
        self.patch(index, operations)
    }

    /// The project's page server: the one Service labelled [`PAGE_SERVER_LABEL`], in the project
    /// or among the objects this plan creates.
    pub(crate) fn page_server(&self) -> Result<PageServer, PlanError> {
        let requested = self
            .requested
            .iter()
            .filter(|o| {
                o.get("apiVersion").and_then(Value::as_str) == Some("v1")
                    && o.get("kind").and_then(Value::as_str) == Some("Service")
            })
            .filter_map(|o| serde_json::from_value::<Service>(o.clone()).ok());
        let labelled: Vec<Service> = self
            .input
            .snapshot
            .services
            .iter()
            .cloned()
            .chain(requested)
            .filter(|s| is_page_server(&s.metadata))
            .collect();
        match labelled.as_slice() {
            [] => Err(no_page_server()),
            [one] => page_server(one).ok_or_else(|| {
                error(
                    ErrorKind::Invalid,
                    "page_server_ports",
                    format!(
                        "Service {} is labelled {PAGE_SERVER_LABEL}=true but has no ports named \
                         {PAGES_PORT} and {MAINTENANCE_PORT}.",
                        one.metadata.name.as_deref().unwrap_or_default()
                    ),
                )
            }),
            many => {
                let names: Vec<String> = many
                    .iter()
                    .filter_map(|s| s.metadata.name.clone())
                    .collect();
                Err(several_page_servers("Service", &names))
            }
        }
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
        let server = self.page_server()?;
        if middleware.is_none() {
            let down = maintenance(&meta).ok_or_else(|| bad_annotation(ingress))?;
            let spec = errors_spec(&server, down.keys());
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
        workload: Option<&WorkloadRef>,
    ) -> Result<(), PlanError> {
        let object = self.checked_route(route, hostname)?;
        let (route, object) = match route.kind {
            RouteKind::Ingress => (route.clone(), object),
            RouteKind::HttpRoute => {
                let (name, object) = self.own_http_route(&route.name, object, hostname)?;
                let route = RouteRef {
                    kind: RouteKind::HttpRoute,
                    name,
                };
                (route, object)
            }
        };
        let route = &route;
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
        let server = if enabled {
            Some(self.page_server()?)
        } else {
            self.page_server().ok()
        };
        self.claim(index, &[Step::field("spec"), Step::field("rules")], "rules")?;
        let rules = array(&current, "/spec/rules");
        let swapped = match route.kind {
            RouteKind::Ingress => {
                ingress_swap(&rules, hostname, kept.get(hostname), server.as_ref())
            }
            RouteKind::HttpRoute => http_route_swap(&rules, kept.get(hostname), server.as_ref()),
        };
        let (operations, had) = match (swapped, workload) {
            (Err(e), Some(workload)) if !enabled && e.code == "maintenance_changed" => {
                let page = server.as_ref().ok_or_else(no_page_server)?.name.clone();
                let (service, port) = self.first_endpoint(workload, route)?;
                self.impact.insert(workload.clone());
                self.warnings.push(Warning::MaintenanceRestored {
                    hostname: hostname.to_owned(),
                    service: service.clone(),
                    port,
                });
                let operations = match route.kind {
                    RouteKind::Ingress => ingress_back(&rules, hostname, &page, &service, port),
                    RouteKind::HttpRoute => http_route_back(&rules, &page, &service, port),
                };
                (operations, Value::Null)
            }
            (swapped, _) => swapped?,
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
            self.retune_errors(&route.name, &query(kept.keys()))?;
        }
        Ok(())
    }

    /// Sets the query of the errors middleware of Ingress `ingress`, where it exists.
    fn retune_errors(&mut self, ingress: &str, query: &str) -> Result<(), PlanError> {
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
        let current = self.current(index);
        let mut operations = Vec::new();
        for (path, after) in [
            ("/spec/errors/status", json!(["502-504"])),
            ("/spec/errors/query", json!(query)),
        ] {
            match current.pointer(path) {
                Some(before) if *before == after => {}
                Some(before) => {
                    operations.extend([test(path, before.clone()), replace(path, after)]);
                }
                None => operations.push(add(path, after)),
            }
        }
        self.patch(index, operations)
    }
}

/// The patch that sends the paths of `hostname` to the maintenance port, or back to `kept`, and
/// the backends they had.
///
/// `server` is needed when the hostname starts maintenance, so when nothing is kept.
fn ingress_swap(
    rules: &[Value],
    hostname: &str,
    kept: Option<&Value>,
    server: Option<&PageServer>,
) -> Result<(Vec<Value>, Value), PlanError> {
    let page =
        server.map(|s| json!({"service": {"name": s.name, "port": {"number": s.maintenance}}}));
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
            let next = match (&back, &page) {
                (Some(back), _) => back.get(had.len()).cloned().ok_or_else(changed)?,
                (None, Some(page)) => page.clone(),
                (None, None) => return Err(no_page_server()),
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
    server: Option<&PageServer>,
) -> Result<(Vec<Value>, Value), PlanError> {
    let back = kept.map(elements);
    let page = match (&back, server) {
        (Some(_), _) => Value::Null,
        (None, Some(s)) => json!([{"name": s.name, "port": s.maintenance}]),
        (None, None) => return Err(no_page_server()),
    };
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

/// The patch that sends the paths of `hostname` that show page server `page`'s maintenance page
/// to `service`.
fn ingress_back(
    rules: &[Value],
    hostname: &str,
    page: &str,
    service: &str,
    port: i32,
) -> Vec<Value> {
    let back = json!({"service": {"name": service, "port": {"number": port}}});
    let mut operations = Vec::new();
    for (r, rule) in rules.iter().enumerate() {
        if rule.get("host").and_then(Value::as_str) != Some(hostname) {
            continue;
        }
        for (p, path) in array(rule, "/http/paths").iter().enumerate() {
            let Some(backend) = path
                .get("backend")
                .filter(|b| shows_page(b, "/service/name", page))
            else {
                continue;
            };
            let at = format!("/spec/rules/{r}/http/paths/{p}/backend");
            operations.extend([test(&at, backend.clone()), replace(&at, back.clone())]);
        }
    }
    operations
}

/// As [`ingress_back`], for the `backendRefs` of each rule of an `HTTPRoute`.
fn http_route_back(rules: &[Value], page: &str, service: &str, port: i32) -> Vec<Value> {
    let back = json!([{"name": service, "port": port}]);
    let mut operations = Vec::new();
    for (r, rule) in rules.iter().enumerate() {
        let Some(refs) = rule
            .get("backendRefs")
            .filter(|refs| elements(refs).iter().any(|b| shows_page(b, "/name", page)))
        else {
            continue;
        };
        let at = format!("/spec/rules/{r}/backendRefs");
        operations.extend([test(&at, refs.clone()), replace(&at, back.clone())]);
    }
    operations
}

fn shows_page(backend: &Value, name: &str, page: &str) -> bool {
    backend.pointer(name).and_then(Value::as_str) == Some(page)
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
             fits them. Name the workload to send the hostname to its first Service."
        ),
    )
}
