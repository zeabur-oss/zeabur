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
//! - **Maintenance on an Ingress** is a second Ingress for the hostname, with path `/` to the
//!   maintenance port. Its Traefik `router.priority` is one more than the highest of the hostname's
//!   routers, so it wins over every path [traefik-priority]. It copies the `router.*` annotations
//!   of the Ingress (TLS, entry points) but not its middlewares, and the same `tls` entry. The
//!   original Ingress owns it, so the garbage collector deletes it with the Ingress. Stopping
//!   deletes it. The workload keeps running.
//! - **Maintenance on an `HTTPRoute`** sets the weight of each `backendRef` to 0 and adds the
//!   maintenance port with weight 1, so every request goes to the maintenance port [gw-weight].
//!   Stopping removes the maintenance port and sets the weights back to 1. A route that already
//!   splits by weight is refused. A filter or a weight applies to every hostname of the route, so a
//!   hostname that shares one moves to a route of its own first, as for a redirect.
//!
//! [k8s-configmap]: https://kubernetes.io/docs/concepts/configuration/configmap/#mounted-configmaps-are-updated-automatically
//! [traefik-errors]: https://doc.traefik.io/traefik/reference/routing-configuration/http/middlewares/errorpages/
//! [traefik-priority]: https://doc.traefik.io/traefik/reference/routing-configuration/kubernetes/ingress/#annotations
//! [gw-weight]: https://gateway-api.sigs.k8s.io/reference/spec/#httpbackendref

use k8s_openapi::api::core::v1::{ConfigMap, Service};
use serde_json::{Map, Value, json};
use zeabur_adapters::error_pages::{
    MAINTENANCE_PORT, PAGE_SERVER_LABEL, PAGES_PORT, PageServer, is_page_server, maintenance_of,
    page_server,
};
use zeabur_adapters::traefik;

use crate::access::{meta_of, middleware_resource, owner_of, redirect_rule};
use crate::change::{Change, ErrorPageHtml, RouteKind, RouteRef, Target};
use crate::checks::ResourceRef;
use crate::fields::Step;
use crate::json::{add, pointer, remove, replace, test};
use crate::plan::{
    ErrorKind, PlanError, Planner, array, error, http_route_target, ingress_target, to_json,
};
use crate::rules::route_name;

/// The prefix of the Traefik annotations on an Ingress that set up its router.
const ROUTER_PREFIX: &str = "traefik.ingress.kubernetes.io/router.";
const PRIORITY: &str = "traefik.ingress.kubernetes.io/router.priority";
const NAME_MAX: usize = 253;

/// The largest page, so that the ConfigMap stays far below its limit of 1 MiB.
const PAGE_MAX: usize = 64 * 1024;

/// The Traefik `Middleware` a change reads first, if any: the plan updates or deletes it.
pub(crate) fn middleware_read(change: &Change) -> Option<(ResourceRef, String)> {
    let Change::ErrorPages(pages) = change else {
        return None;
    };
    (pages.route.kind == RouteKind::Ingress).then(|| {
        (
            middleware_resource(),
            traefik::error_pages_name(&pages.route.name),
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

/// The `errors` middleware spec of an Ingress.
pub(crate) fn errors_spec(server: &PageServer) -> Value {
    json!({"errors": {
        "status": ["502-504"],
        "service": {"name": server.name, "port": server.pages},
        "query": "/{status}.html",
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
            self.created_middleware(&name, owner_of(&meta).as_ref(), &errors_spec(&server));
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
        let shown = match route.kind {
            RouteKind::Ingress => self.maintenance_ingress(&route.name, hostname).is_some(),
            RouteKind::HttpRoute => shows_maintenance(&object, self.page_server().ok().as_ref()),
        };
        match (enabled, shown) {
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
        match (route.kind, enabled) {
            (RouteKind::Ingress, true) => {
                let server = self.page_server()?;
                self.refuse_without_traefik(&object, &route.name)?;
                self.start_ingress_maintenance(&route.name, &object, hostname, &server)
            }
            (RouteKind::Ingress, false) => self.end_maintenance_ingress(route, hostname),
            (RouteKind::HttpRoute, true) => {
                if redirect_rule(&object).is_some() {
                    return Err(error(
                        ErrorKind::Conflict,
                        "redirecting",
                        format!(
                            "{hostname} redirects, so nobody would see the maintenance page; stop the redirect first."
                        ),
                    ));
                }
                let server = self.page_server()?;
                let (name, object) = self.own_http_route(&route.name, object, hostname)?;
                self.weigh_backends(&name, &object, &server)
            }
            (RouteKind::HttpRoute, false) => {
                let server = self.page_server().ok();
                self.restore_backends(&route.name, &object, server.as_ref())
            }
        }
    }

    /// The maintenance Ingress of Ingress `owner` for `hostname`, by name.
    pub(crate) fn maintenance_ingress(&self, owner: &str, hostname: &str) -> Option<String> {
        self.input.snapshot.ingresses.iter().find_map(|i| {
            let (of, hosts) = maintenance_of(i)?;
            (of == owner && hosts.iter().any(|h| h == hostname))
                .then(|| i.metadata.name.clone())
                .flatten()
        })
    }

    /// Deletes the maintenance Ingress of `route` for `hostname`, when it has one.
    pub(crate) fn end_maintenance_ingress(
        &mut self,
        route: &RouteRef,
        hostname: &str,
    ) -> Result<(), PlanError> {
        if route.kind != RouteKind::Ingress {
            return Ok(());
        }
        self.maintenance_ingress(&route.name, hostname)
            .map_or(Ok(()), |name| {
                self.delete(&RouteKind::Ingress.resource(), &name, None)
            })
    }

    fn start_ingress_maintenance(
        &mut self,
        ingress: &str,
        object: &Value,
        hostname: &str,
        server: &PageServer,
    ) -> Result<(), PlanError> {
        let meta = meta_of(object);
        let owner = owner_of(&meta).ok_or_else(|| {
            error(
                ErrorKind::Invalid,
                "unreadable_object",
                format!("Ingress {ingress} has no uid to own its maintenance Ingress."),
            )
        })?;
        let services = RouteKind::Ingress.services(object);
        let workload = self.behind(&services).into_iter().next();
        let mut name = route_name(workload.as_ref().map_or(ingress, |w| &w.name), hostname);
        name.truncate(NAME_MAX - SUFFIX.len());
        name.push_str(SUFFIX);
        if self
            .input
            .snapshot
            .ingresses
            .iter()
            .any(|i| i.metadata.name.as_deref() == Some(name.as_str()))
        {
            return Err(error(
                ErrorKind::Conflict,
                "route_name_taken",
                format!("A route named {name} already exists."),
            ));
        }
        let mut annotations: Map<String, Value> = meta
            .annotations
            .iter()
            .flatten()
            .filter(|(key, _)| {
                key.starts_with(ROUTER_PREFIX)
                    && key.as_str() != traefik::MIDDLEWARES_ANNOTATION
                    && key.as_str() != PRIORITY
            })
            .map(|(key, value)| (key.clone(), json!(value)))
            .collect();
        annotations.insert(
            PRIORITY.to_owned(),
            json!((self.host_priority(hostname) + 1).to_string()),
        );
        let mut spec = json!({"rules": [{"host": hostname, "http": {"paths": [{
            "path": "/",
            "pathType": "Prefix",
            "backend": {"service": {"name": server.name, "port": {"name": MAINTENANCE_PORT}}},
        }]}}]});
        if let Some(map) = spec.as_object_mut() {
            if let Some(class) = object.pointer("/spec/ingressClassName") {
                map.insert("ingressClassName".to_owned(), class.clone());
            }
            let tls = array(object, "/spec/tls").into_iter().find(|t| {
                array(t, "/hosts")
                    .iter()
                    .any(|h| h.as_str() == Some(hostname))
            });
            if let Some(tls) = tls {
                let mut entry = json!({"hosts": [hostname]});
                if let (Some(secret), Some(entry)) = (tls.get("secretName"), entry.as_object_mut())
                {
                    entry.insert("secretName".to_owned(), secret.clone());
                }
                map.insert("tls".to_owned(), json!([entry]));
            }
        }
        let mut metadata = self.owned_metadata(&name, Some(&owner));
        if let Some(map) = metadata.as_object_mut() {
            map.insert("annotations".to_owned(), Value::Object(annotations));
        }
        self.created(
            ingress_target(&name),
            json!({
                "apiVersion": "networking.k8s.io/v1",
                "kind": "Ingress",
                "metadata": metadata,
                "spec": spec,
            }),
        );
        Ok(())
    }

    /// The highest Traefik priority of the project's Ingress routers that match `hostname`.
    ///
    /// A router has the priority its Ingress names in `router.priority`. Else Traefik uses the
    /// length of the rule it builds, such as ``Host(`a.example.com`) && PathPrefix(`/`)``.
    fn host_priority(&self, hostname: &str) -> u64 {
        let mut highest = 0;
        for ingress in &self.input.snapshot.ingresses {
            let named = ingress
                .metadata
                .annotations
                .as_ref()
                .and_then(|a| a.get(PRIORITY))
                .and_then(|p| p.parse::<u64>().ok())
                .filter(|p| *p > 0);
            for rule in ingress.spec.iter().flat_map(|s| s.rules.iter().flatten()) {
                let host = rule.host.as_deref().unwrap_or_default();
                if !host.is_empty() && !zeabur_adapters::gateway::intersects(host, hostname) {
                    continue;
                }
                for path in rule.http.iter().flat_map(|h| &h.paths) {
                    let length = rule_length(host, path.path.as_deref(), &path.path_type);
                    highest = highest.max(named.unwrap_or(length));
                }
            }
        }
        highest
    }

    /// Sets the `backendRefs` of `HTTPRoute` `name` to weight 0 and adds the maintenance port of
    /// `server` with weight 1.
    fn weigh_backends(
        &mut self,
        name: &str,
        object: &Value,
        server: &PageServer,
    ) -> Result<(), PlanError> {
        let mut operations = Vec::new();
        for (r, rule) in array(object, "/spec/rules").iter().enumerate() {
            let refs = array(rule, "/backendRefs");
            if refs.is_empty() {
                continue;
            }
            if refs
                .iter()
                .any(|b| b.get("weight").is_some_and(|w| *w != json!(1)))
            {
                return Err(error(
                    ErrorKind::Conflict,
                    "weighted_backends",
                    format!(
                        "HTTPRoute {name} splits its requests by weight; show the maintenance page with kubectl."
                    ),
                ));
            }
            for (b, backend) in refs.iter().enumerate() {
                let path = format!("/spec/rules/{r}/backendRefs/{b}/weight");
                match backend.get("weight") {
                    Some(weight) => {
                        operations.extend([test(&path, weight.clone()), replace(&path, json!(0))]);
                    }
                    None => operations.push(add(&path, json!(0))),
                }
            }
            operations.push(add(
                &format!("/spec/rules/{r}/backendRefs/-"),
                json!({"name": server.name, "port": server.maintenance, "weight": 1}),
            ));
        }
        if operations.is_empty() {
            return Err(error(
                ErrorKind::Invalid,
                "no_backend",
                "The hostname sends its requests to no Service.",
            ));
        }
        let index = self.working(http_route_target(name), object.clone());
        self.claim(index, &[Step::field("spec"), Step::field("rules")], "rules")?;
        self.patch(index, operations)
    }

    /// Removes the page server's `backendRefs` from `HTTPRoute` `name` and sets the others back to
    /// weight 1.
    fn restore_backends(
        &mut self,
        name: &str,
        object: &Value,
        server: Option<&PageServer>,
    ) -> Result<(), PlanError> {
        let mut operations = Vec::new();
        let mut removals = Vec::new();
        for (r, rule) in array(object, "/spec/rules").iter().enumerate() {
            let refs = array(rule, "/backendRefs");
            let page = page_refs(&refs, server);
            for (b, backend) in refs.iter().enumerate() {
                let path = format!("/spec/rules/{r}/backendRefs/{b}");
                if page.contains(&b) {
                    removals.push((path, backend.clone()));
                } else if !page.is_empty() && backend.get("weight") == Some(&json!(0)) {
                    let path = format!("{path}/weight");
                    operations.extend([test(&path, json!(0)), replace(&path, json!(1))]);
                }
            }
        }
        // Remove from the end, so the paths of the next removals stay valid.
        for (path, backend) in removals.into_iter().rev() {
            operations.extend([test(&path, backend), remove(&path)]);
        }
        let index = self.working(http_route_target(name), object.clone());
        self.claim(index, &[Step::field("spec"), Step::field("rules")], "rules")?;
        self.patch(index, operations)
    }
}

/// What a maintenance Ingress adds to its name.
const SUFFIX: &str = "-maintenance";

/// The length of the rule Traefik builds for an Ingress path of `host`.
fn rule_length(host: &str, path: Option<&str>, path_type: &str) -> u64 {
    let host_rule = if host.is_empty() {
        String::new()
    } else if let Some(suffix) = host.strip_prefix("*.") {
        // Traefik escapes the suffix in a regular expression: a generous upper bound.
        format!(
            "HostRegexp(`^[a-z0-9-\\.]+\\.{}$`)",
            suffix.replace('.', "\\\\.")
        )
    } else {
        format!("Host(`{host}`)")
    };
    let path_rule = path.map(|p| match path_type {
        "Exact" => format!("Path(`{p}`)"),
        _ => format!("PathPrefix(`{p}`)"),
    });
    let rule = match (host_rule.is_empty(), path_rule) {
        (false, Some(path)) => format!("{host_rule} && {path}"),
        (true, Some(path)) => path,
        (_, None) => host_rule,
    };
    u64::try_from(rule.len()).unwrap_or(u64::MAX)
}

/// Whether a rule of `HTTPRoute` `object` sends to the maintenance port of `server`.
pub(crate) fn shows_maintenance(object: &Value, server: Option<&PageServer>) -> bool {
    array(object, "/spec/rules")
        .iter()
        .any(|rule| !page_refs(&array(rule, "/backendRefs"), server).is_empty())
}

/// The indexes of the `backendRefs` among `refs` that send to the maintenance port of `server`.
/// Without a page server, it is the one of weight 1 next to others of weight 0.
fn page_refs(refs: &[Value], server: Option<&PageServer>) -> Vec<usize> {
    let weight = |b: &Value| b.get("weight").and_then(Value::as_i64).unwrap_or(1);
    refs.iter()
        .enumerate()
        .filter(|(b, backend)| {
            server.map_or_else(
                || {
                    refs.len() > 1
                        && weight(backend) == 1
                        && refs
                            .iter()
                            .enumerate()
                            .all(|(o, other)| o == *b || weight(other) == 0)
                },
                |server| {
                    backend.get("name").and_then(Value::as_str) == Some(server.name.as_str())
                        && backend.get("port").and_then(Value::as_i64)
                            == Some(i64::from(server.maintenance))
                },
            )
        })
        .map(|(b, _)| b)
        .collect()
}
