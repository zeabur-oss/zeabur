//! Redirects and basic auth on a domain.
//!
//! - **On an Ingress** they are Traefik `Middleware` objects, listed in the Ingress annotation
//!   `traefik.ingress.kubernetes.io/router.middlewares` [traefik-mw]. Traefik must serve the kind,
//!   and the Ingress class, when it has one, must be Traefik's. A redirect is a `redirectRegex`
//!   that matches only its hostname, so the other hostnames of the Ingress keep their requests.
//!   Basic auth applies to every hostname of the Ingress. Its users are htpasswd lines with bcrypt
//!   hashes in a Secret; the password itself is not stored. The Ingress owns the middlewares and
//!   the Secret, so they go when it goes.
//! - **On an `HTTPRoute`** a redirect is a rule with a `RequestRedirect` filter [gw-redirect],
//!   first in `spec.rules`. A filter applies to every hostname of the route, so a hostname that
//!   shares its route first moves to a new route of its own, with the same parents and rules, named
//!   as "Add domain" names one. Gateway API refuses the filter next to `backendRefs` in one rule,
//!   so the redirect rule has none. It copies the `matches` of every other rule: of rules that
//!   match a request equally, the first in list order wins [gw-precedence]. The other rules stay,
//!   and stopping the redirect removes the redirect rule. Gateway API has no basic auth filter, so
//!   basic auth on an `HTTPRoute` is refused.
//!
//! [traefik-mw]: https://doc.traefik.io/traefik/reference/routing-configuration/kubernetes/ingress/
//! [gw-redirect]: https://gateway-api.sigs.k8s.io/reference/spec/#httprequestredirectfilter
//! [gw-precedence]: https://gateway-api.sigs.k8s.io/reference/spec/#httprouterule

use base64::Engine as _;
use k8s_openapi::api::core::v1::Secret;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use zeabur_adapters::traefik::{self, MIDDLEWARES_ANNOTATION};

use crate::change::{BasicAuthUser, Change, ObjectName, RouteKind, RouteRef, Target, Warning};
use crate::checks::ResourceRef;
use crate::edit::LAST_APPLIED;
use crate::fields::Step;
use crate::json::{add, pointer, remove, replace, test};
use crate::plan::{
    ErrorKind, PlanError, Planner, Working, array, error, http_route_target, ingress_target,
    to_json,
};
use crate::rules::validate_hostname;

const TRAEFIK_GROUP: &str = "traefik.io";
const MIDDLEWARE_VERSION: &str = "v1alpha1";
/// Traefik checks the hash on each request, so a higher cost slows every page.
const BCRYPT_COST: u32 = 10;
/// The most rules, matches in one rule and matches in all rules of an `HTTPRoute` (Gateway API
/// v1.6 CRD validation).
const RULES_MAX: usize = 16;
const RULE_MATCHES_MAX: usize = 64;
const MATCHES_MAX: usize = 128;

pub(crate) fn middleware_resource() -> ResourceRef {
    ResourceRef {
        group: TRAEFIK_GROUP.to_owned(),
        version: MIDDLEWARE_VERSION.to_owned(),
        resource: "middlewares".to_owned(),
    }
}

pub(crate) fn middleware_target(name: &str) -> Target {
    Target {
        api_version: format!("{TRAEFIK_GROUP}/{MIDDLEWARE_VERSION}"),
        kind: "Middleware".to_owned(),
        plural: "middlewares".to_owned(),
        name: name.to_owned(),
    }
}

/// The Traefik `Middleware` a change reads first, if any: the plan updates or deletes it.
#[must_use]
pub fn middleware_read(change: &Change) -> Option<(ResourceRef, String)> {
    if let Change::Redirect(redirect) = change
        && redirect.route.kind == RouteKind::Ingress
    {
        return Some((
            middleware_resource(),
            traefik::redirect_name(&redirect.route.name, &redirect.hostname),
        ));
    }
    if let Change::BasicAuth(auth) = change
        && auth.route.kind == RouteKind::Ingress
    {
        return Some((
            middleware_resource(),
            traefik::basic_auth_name(&auth.route.name),
        ));
    }
    crate::ip_limits::middleware_read(change)
        .or_else(|| crate::error_pages::middleware_read(change))
}

/// The `NetworkPolicy` a change reads first, if any: the plan updates or deletes it.
#[must_use]
pub fn policy_read(change: &Change) -> Option<(ResourceRef, String)> {
    crate::ip_limits::policy_read(change)
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "the planner's change kinds are split across modules by area"
)]
impl Planner<'_> {
    pub(crate) fn redirect(
        &mut self,
        route: &RouteRef,
        hostname: &str,
        to: Option<&str>,
        permanent: bool,
    ) -> Result<(), PlanError> {
        let to = to.map(str::to_ascii_lowercase);
        if hostname.starts_with("*.") || to.as_deref().is_some_and(|t| t.starts_with("*.")) {
            return Err(error(
                ErrorKind::BadRequest,
                "wildcard_redirect",
                "A redirect needs one hostname on each side, not a wildcard.",
            ));
        }
        if let Some(to) = &to {
            validate_hostname(to).map_err(|r| {
                error(
                    ErrorKind::BadRequest,
                    &r.0,
                    format!("{to:?} is not a hostname."),
                )
            })?;
            if to == hostname {
                return Err(error(
                    ErrorKind::BadRequest,
                    "redirect_loop",
                    format!("{hostname} cannot redirect to itself."),
                ));
            }
        }
        let object = self.access_route(route, hostname)?;
        match route.kind {
            RouteKind::Ingress => {
                self.redirect_ingress(&route.name, &object, hostname, to.as_deref(), permanent)?;
            }
            RouteKind::HttpRoute => {
                let (name, object) = self.own_http_route(&route.name, object, hostname)?;
                self.redirect_http_route(&name, &object, to.as_deref(), permanent)?;
            }
        }
        if to.is_some() && permanent {
            self.warnings.push(Warning::PermanentRedirect {
                hostname: hostname.to_owned(),
            });
        }
        Ok(())
    }

    pub(crate) fn basic_auth(
        &mut self,
        route: &RouteRef,
        user: Option<&BasicAuthUser>,
    ) -> Result<(), PlanError> {
        if route.kind == RouteKind::HttpRoute {
            return Err(error(
                ErrorKind::Invalid,
                "basic_auth_needs_ingress",
                "Gateway API has no basic auth filter; add basic auth to a domain on an Ingress.",
            ));
        }
        if let Some(user) = user {
            check_user(user)?;
        }
        let object = self.access_route(route, "")?;
        let meta = meta_of(&object);
        let ingress = route.name.as_str();
        let name = traefik::basic_auth_name(ingress);
        let listed = traefik::middlewares(&meta).contains(&name);
        let middleware = self.fetched_middleware(&name);
        let secret = self.input.snapshot.secret(&name).map(to_json).transpose()?;
        let Some(user) = user else {
            if !listed && middleware.is_none() && secret.is_none() {
                return Err(error(
                    ErrorKind::NotFound,
                    "no_basic_auth",
                    format!("Ingress {ingress} asks for no password."),
                ));
            }
            // The Ingress stops listing the middleware before it goes.
            let index = self.working(ingress_target(ingress), object);
            if listed {
                self.unlist(index, &name)?;
            }
            if middleware.is_some() {
                self.delete(&middleware_resource(), &name, None)?;
            }
            if secret.is_some() {
                let secrets = ResourceRef {
                    group: String::new(),
                    version: "v1".to_owned(),
                    resource: "secrets".to_owned(),
                };
                self.delete(&secrets, &name, None)?;
            }
            return Ok(());
        };
        let line = htpasswd(user, &salt(self.input.now, ingress, &user.username))?;
        let data =
            json!({"users": base64::engine::general_purpose::STANDARD.encode(format!("{line}\n"))});
        let owner = owner_of(&meta);
        if let Some(secret) = secret {
            let index = self.working(Target::of::<Secret>(&name), secret);
            self.patch(index, vec![add("/data", data)])?;
        } else {
            self.created(
                Target::of::<Secret>(&name),
                json!({
                    "apiVersion": "v1",
                    "kind": "Secret",
                    "metadata": self.owned_metadata(&name, owner.as_ref()),
                    "type": "Opaque",
                    "data": data,
                }),
            );
        }
        if middleware.is_none() {
            self.created_middleware(
                &name,
                owner.as_ref(),
                &json!({"basicAuth": {"secret": name, "removeHeader": true}}),
            );
        }
        let index = self.working(ingress_target(ingress), object.clone());
        if !listed {
            self.list(index, &name, false)?;
        }
        let tls: Vec<String> = array(&object, "/spec/tls")
            .iter()
            .flat_map(|t| array(t, "/hosts"))
            .filter_map(|h| h.as_str().map(str::to_owned))
            .collect();
        let plain: Vec<String> = RouteKind::Ingress
            .hosts(&object)
            .into_iter()
            .filter(|h| !tls.contains(h))
            .collect();
        if !plain.is_empty() {
            self.warnings.push(Warning::PasswordOverHttp {
                route: ingress.to_owned(),
                hostnames: plain,
            });
        }
        Ok(())
    }

    /// Route `route` as read, after the checks both changes share. An empty `hostname` is not
    /// checked.
    pub(crate) fn access_route(
        &mut self,
        route: &RouteRef,
        hostname: &str,
    ) -> Result<Value, PlanError> {
        let object = self.checked_route(route, hostname)?;
        if route.kind == RouteKind::Ingress {
            self.refuse_without_traefik(&object, &route.name)?;
        }
        Ok(object)
    }

    /// Route `route` as read, after it is found not managed by a tool and to route `hostname`
    /// (when not empty). Its workloads join the plan's impact.
    pub(crate) fn checked_route(
        &mut self,
        route: &RouteRef,
        hostname: &str,
    ) -> Result<Value, PlanError> {
        let (object, meta, services) = self.route(route)?;
        Self::refuse_managed(&meta, route.kind.kind_name(), &route.name)?;
        if !hostname.is_empty() && !route.kind.hosts(&object).iter().any(|h| h == hostname) {
            return Err(error(
                ErrorKind::NotFound,
                "unknown_host",
                format!("{} does not route {hostname}.", route.name),
            ));
        }
        let behind = self.behind(&services);
        self.impact.extend(behind);
        Ok(object)
    }

    pub(crate) fn refuse_without_traefik(
        &self,
        ingress: &Value,
        name: &str,
    ) -> Result<(), PlanError> {
        let served = self
            .input
            .kinds
            .iter()
            .any(|k| k.group == TRAEFIK_GROUP && k.kind == "Middleware");
        let class = ingress
            .pointer("/spec/ingressClassName")
            .or_else(|| ingress.pointer("/metadata/annotations/kubernetes.io~1ingress.class"))
            .and_then(Value::as_str);
        if served && class.is_none_or(|c| c.contains("traefik")) {
            return Ok(());
        }
        Err(error(
            ErrorKind::Invalid,
            "needs_traefik",
            format!(
                "Redirects, passwords, IP limits and error pages on Ingress {name} need Traefik, and {}.",
                if served {
                    "its class is not Traefik's"
                } else {
                    "this cluster does not serve Traefik middlewares"
                }
            ),
        ))
    }

    fn redirect_ingress(
        &mut self,
        ingress: &str,
        object: &Value,
        hostname: &str,
        to: Option<&str>,
        permanent: bool,
    ) -> Result<(), PlanError> {
        let meta = meta_of(object);
        let name = traefik::redirect_name(ingress, hostname);
        let listed = traefik::middlewares(&meta).contains(&name);
        let middleware = self.fetched_middleware(&name);
        let Some(to) = to else {
            if !listed && middleware.is_none() {
                return Err(error(
                    ErrorKind::NotFound,
                    "not_redirected",
                    format!("{hostname} does not redirect."),
                ));
            }
            let index = self.working(ingress_target(ingress), object.clone());
            if listed {
                self.unlist(index, &name)?;
            }
            if middleware.is_some() {
                self.delete(&middleware_resource(), &name, None)?;
            }
            return Ok(());
        };
        let spec = json!({"redirectRegex": {
            "regex": format!("^https?://{}(:[0-9]+)?/(.*)", hostname.replace('.', "\\.")),
            "replacement": format!("https://{to}/${{2}}"),
            "permanent": permanent,
        }});
        if let Some(found) = middleware {
            let index = self.working(middleware_target(&name), found);
            self.patch(index, vec![add("/spec", spec)])?;
        } else {
            self.created_middleware(&name, owner_of(&meta).as_ref(), &spec);
        }
        let index = self.working(ingress_target(ingress), object.clone());
        if !listed {
            // Before basic auth, so a redirect asks for no password.
            self.list(index, &name, true)?;
        }
        Ok(())
    }

    fn redirect_http_route(
        &mut self,
        name: &str,
        object: &Value,
        to: Option<&str>,
        permanent: bool,
    ) -> Result<(), PlanError> {
        let rules = array(object, "/spec/rules");
        let found = redirect_rule(object);
        let index = self.working(http_route_target(name), object.clone());
        self.claim(index, &[Step::field("spec"), Step::field("rules")], "rules")?;
        let Some(to) = to else {
            let Some(at) = found else {
                return Err(error(
                    ErrorKind::NotFound,
                    "not_redirected",
                    format!("HTTPRoute {name} does not redirect."),
                ));
            };
            let path = format!("/spec/rules/{at}");
            let rule = rules.get(at).cloned().unwrap_or_default();
            return self.patch(index, vec![test(&path, rule), remove(&path)]);
        };
        let filters = json!([{"type": "RequestRedirect", "requestRedirect": {
            "scheme": "https",
            "hostname": to,
            "statusCode": if permanent { 301 } else { 302 },
        }}]);
        if let Some(at) = found {
            let path = format!("/spec/rules/{at}/filters");
            let before = rules
                .get(at)
                .and_then(|r| r.get("filters"))
                .cloned()
                .unwrap_or_default();
            return self.patch(index, vec![test(&path, before), replace(&path, filters)]);
        }
        // A rule without matches matches every path, as a `PathPrefix` of `/`.
        let every = json!({"path": {"type": "PathPrefix", "value": "/"}});
        let mut matches: Vec<Value> = Vec::new();
        for rule in &rules {
            let own = array(rule, "/matches");
            for entry in if own.is_empty() {
                vec![every.clone()]
            } else {
                own
            } {
                if !matches.contains(&entry) {
                    matches.push(entry);
                }
            }
        }
        let total: usize = rules
            .iter()
            .map(|r| array(r, "/matches").len().max(1))
            .sum::<usize>()
            + matches.len();
        if rules.len() >= RULES_MAX || matches.len() > RULE_MATCHES_MAX || total > MATCHES_MAX {
            return Err(error(
                ErrorKind::Invalid,
                "too_many_rules",
                format!(
                    "HTTPRoute {name} has too many rules or matches for a redirect rule that copies them; add the redirect with kubectl."
                ),
            ));
        }
        let rule = json!({"matches": matches, "filters": filters});
        let operation = if object.pointer("/spec/rules").is_some() {
            add("/spec/rules/0", rule)
        } else {
            add("/spec/rules", json!([rule]))
        };
        self.patch(index, vec![operation])
    }

    /// `HTTPRoute` `name` as read, when `hostname` is its only hostname. Otherwise the hostname
    /// moves to a new route of its own, as "Add domain" names one, with the same parents and
    /// rules: a filter applies to every hostname of a route.
    pub(crate) fn own_http_route(
        &mut self,
        name: &str,
        object: Value,
        hostname: &str,
    ) -> Result<(String, Value), PlanError> {
        if array(&object, "/spec/hostnames").len() <= 1 {
            return Ok((name.to_owned(), object));
        }
        let services = array(&object, "/spec/rules")
            .iter()
            .flat_map(|rule| array(rule, "/backendRefs"))
            .filter_map(|r| r.get("name").and_then(Value::as_str).map(str::to_owned))
            .collect();
        let own = self.own_route_name(&services, name, hostname)?;
        let meta = meta_of(&object);
        let mut annotations = meta.annotations.clone().unwrap_or_default();
        annotations.remove(LAST_APPLIED);
        let mut metadata = json!({"name": own, "namespace": self.input.snapshot.namespace});
        if let Some(map) = metadata.as_object_mut() {
            if let Some(labels) = &meta.labels {
                map.insert("labels".to_owned(), json!(labels));
            }
            if !annotations.is_empty() {
                map.insert("annotations".to_owned(), json!(annotations));
            }
        }
        let mut spec = object.get("spec").cloned().unwrap_or_default();
        if let Some(map) = spec.as_object_mut() {
            map.insert("hostnames".to_owned(), json!([hostname]));
        }
        let body = json!({
            "apiVersion": "gateway.networking.k8s.io/v1",
            "kind": "HTTPRoute",
            "metadata": metadata,
            "spec": spec,
        });
        let route = RouteRef {
            kind: RouteKind::HttpRoute,
            name: name.to_owned(),
        };
        self.remove_domain(&route, hostname)?;
        self.created(http_route_target(&own), body.clone());
        self.warnings.push(Warning::DomainMoved {
            hostname: hostname.to_owned(),
            from: ObjectName::of("HTTPRoute", name),
            to: ObjectName::of("HTTPRoute", &own),
        });
        Ok((own, body))
    }

    pub(crate) fn fetched_middleware(&self, name: &str) -> Option<Value> {
        let resource = middleware_resource();
        self.input
            .objects
            .iter()
            .find(|f| f.resource == resource && f.name == name)
            .map(|f| f.object.clone())
    }

    pub(crate) fn owned_metadata(&self, name: &str, owner: Option<&Value>) -> Value {
        let mut metadata = json!({"name": name, "namespace": self.input.snapshot.namespace});
        if let (Some(owner), Some(map)) = (owner, metadata.as_object_mut()) {
            map.insert("ownerReferences".to_owned(), json!([owner]));
        }
        metadata
    }

    pub(crate) fn created_middleware(&mut self, name: &str, owner: Option<&Value>, spec: &Value) {
        let body = json!({
            "apiVersion": format!("{TRAEFIK_GROUP}/{MIDDLEWARE_VERSION}"),
            "kind": "Middleware",
            "metadata": self.owned_metadata(name, owner),
            "spec": spec,
        });
        self.created(middleware_target(name), body);
    }

    pub(crate) fn created(&mut self, target: Target, body: Value) {
        self.objects.push(Working {
            target,
            original: None,
            current: body,
            operations: Vec::new(),
            owners: Vec::new(),
            deleted: false,
        });
    }

    /// Adds middleware `name` to the annotation of Ingress `index`: first, or last.
    pub(crate) fn list(&mut self, index: usize, name: &str, first: bool) -> Result<(), PlanError> {
        let entry = traefik::reference(&self.input.snapshot.namespace, name);
        self.edit_list(index, |entries| {
            if first {
                entries.insert(0, entry);
            } else {
                entries.push(entry);
            }
        })
    }

    pub(crate) fn unlist(&mut self, index: usize, name: &str) -> Result<(), PlanError> {
        let entry = traefik::reference(&self.input.snapshot.namespace, name);
        self.edit_list(index, |entries| entries.retain(|e| *e != entry))
    }

    fn edit_list(
        &mut self,
        index: usize,
        edit: impl FnOnce(&mut Vec<String>),
    ) -> Result<(), PlanError> {
        let current = self.current(index);
        let path = pointer(&["metadata", "annotations", MIDDLEWARES_ANNOTATION]);
        let mut entries: Vec<String> = current
            .pointer(&path)
            .and_then(Value::as_str)
            .into_iter()
            .flat_map(|list| list.split(','))
            .map(str::trim)
            .filter(|e| !e.is_empty())
            .map(str::to_owned)
            .collect();
        edit(&mut entries);
        self.set_annotation(index, MIDDLEWARES_ANNOTATION, &entries.join(","))
    }

    /// Sets annotation `key` of object `index` to `after`, or removes it when `after` is empty.
    pub(crate) fn set_annotation(
        &mut self,
        index: usize,
        key: &str,
        after: &str,
    ) -> Result<(), PlanError> {
        self.claim(
            index,
            &[
                Step::field("metadata"),
                Step::field("annotations"),
                Step::field(key),
            ],
            "annotations",
        )?;
        let current = self.current(index);
        let path = pointer(&["metadata", "annotations", key]);
        let before = current.pointer(&path).and_then(Value::as_str);
        let operations = match before {
            Some(before) if after.is_empty() => vec![test(&path, json!(before)), remove(&path)],
            Some(before) => vec![test(&path, json!(before)), replace(&path, json!(after))],
            None if current.pointer("/metadata/annotations").is_some() => {
                vec![add(&path, json!(after))]
            }
            None => vec![add("/metadata/annotations", json!({ key: after }))],
        };
        self.patch(index, operations)
    }
}

fn check_user(user: &BasicAuthUser) -> Result<(), PlanError> {
    let refused = |code: &str, detail: &str| {
        let mut refused = error(ErrorKind::BadRequest, code, detail);
        refused.field = Some(
            if code == "invalid_username" {
                "username"
            } else {
                "password"
            }
            .to_owned(),
        );
        Err(refused)
    };
    if user.username.is_empty()
        || user.username.contains(':')
        || user.username.chars().any(char::is_whitespace)
    {
        return refused(
            "invalid_username",
            "A user name needs at least one character, and no colon or space.",
        );
    }
    if !(8..=72).contains(&user.password.len()) {
        return refused(
            "invalid_password",
            "A password needs 8 to 72 bytes; bcrypt reads no more than 72.",
        );
    }
    Ok(())
}

/// The salt is fixed by the plan's time, so the plan and its apply write the same hash.
fn salt(now: &str, ingress: &str, username: &str) -> [u8; 16] {
    let digest = Sha256::digest(format!("{now}\n{ingress}\n{username}"));
    let mut salt = [0; 16];
    salt.copy_from_slice(digest.get(..16).unwrap_or(&[0; 16]));
    salt
}

fn htpasswd(user: &BasicAuthUser, salt: &[u8; 16]) -> Result<String, PlanError> {
    let hash = bcrypt::non_truncating_hash_with_salt(&user.password, BCRYPT_COST, *salt)
        .map_err(|e| error(ErrorKind::BadRequest, "invalid_password", e.to_string()))?;
    Ok(format!(
        "{}:{}",
        user.username,
        hash.format_for_version(bcrypt::Version::TwoY)
    ))
}

/// The index of the redirect rule of `HTTPRoute` `object`: a rule with a `RequestRedirect`
/// filter and no `backendRefs`.
pub(crate) fn redirect_rule(object: &Value) -> Option<usize> {
    array(object, "/spec/rules").iter().position(|rule| {
        array(rule, "/backendRefs").is_empty()
            && array(rule, "/filters")
                .iter()
                .any(|f| f.get("type").and_then(Value::as_str) == Some("RequestRedirect"))
    })
}

pub(crate) fn meta_of(object: &Value) -> ObjectMeta {
    object
        .get("metadata")
        .cloned()
        .and_then(|m| serde_json::from_value(m).ok())
        .unwrap_or_default()
}

/// An owner reference to the Ingress, so the garbage collector deletes what it owns with it.
pub(crate) fn owner_of(meta: &ObjectMeta) -> Option<Value> {
    Some(json!({
        "apiVersion": "networking.k8s.io/v1",
        "kind": "Ingress",
        "name": meta.name.as_deref()?,
        "uid": meta.uid.as_deref()?,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hash_is_one_traefik_reads_and_the_same_on_apply() {
        let user = BasicAuthUser {
            username: "ops".to_owned(),
            password: "correct horse".to_owned(),
        };
        let line = htpasswd(&user, &salt("2026-10-23T10:00:00Z", "web", "ops")).unwrap();
        let again = htpasswd(&user, &salt("2026-10-23T10:00:00Z", "web", "ops")).unwrap();
        assert_eq!(line, again);
        let hash = line.strip_prefix("ops:").unwrap();
        assert!(hash.starts_with("$2y$10$"));
        assert!(bcrypt::verify("correct horse", hash).unwrap());
    }
}
