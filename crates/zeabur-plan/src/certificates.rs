//! Where the certificate of a domain comes from.
//!
//! - **An issuer.** cert-manager reads `cert-manager.io/cluster-issuer` on an Ingress and makes a
//!   `Certificate` for each `spec.tls` entry [cm-ingress]. The annotation is on the Ingress, so a
//!   new issuer applies to every HTTPS hostname of it. Let's Encrypt issues a wildcard only through
//!   the DNS-01 challenge [le-challenges], so a wildcard hostname needs an issuer with a DNS-01
//!   solver, or one that is not ACME.
//! - **The person's own certificate.** It goes into a `kubernetes.io/tls` Secret [k8s-tls] named by
//!   [`own_secret_name`], and the hostname's `tls` entry names it. cert-manager writes the Secret
//!   of every `tls` entry of an Ingress that names an issuer, so the Ingress stops naming one. When
//!   it also serves other hostnames, the hostname moves to a new Ingress of its own, named as "Add
//!   domain" names one. The new Ingress keeps the hostname's rules, class, labels and annotations,
//!   and gets its own error pages when the old one showed them. A password, a redirect or
//!   maintenance belongs to the old Ingress, so the move is refused until they stop. The
//!   `Certificate` that cert-manager made for the hostname's previous Secret is deleted.
//! - Only an Ingress names its certificates. The certificates of an `HTTPRoute` come from the
//!   listener of its Gateway, which is the cluster's, not the project's: such a change is refused.
//!
//! The API checks an own certificate (key, hostname, expiry) before it plans; the plan takes it
//! only as a [`CheckedCertificate`].
//!
//! [cm-ingress]: https://cert-manager.io/docs/usage/ingress/
//! [le-challenges]: https://letsencrypt.org/docs/challenge-types/
//! [k8s-tls]: https://kubernetes.io/docs/concepts/configuration/secret/#tls-secrets

use base64::Engine as _;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use serde_json::{Map, Value, json};
use zeabur_adapters::traefik;

use crate::access::{meta_of, owner_of};
use crate::change::{CertificateSource, ObjectName, RouteKind, RouteRef, Target, Warning};
use crate::checks::ResourceRef;
use crate::edit::LAST_APPLIED;
use crate::error_pages::errors_spec;
use crate::fields::Step;
use crate::json::{add, pointer, remove, replace, test};
use crate::plan::{ErrorKind, PlanError, Planner, Working, array, error, ingress_target, to_json};
use crate::rules::route_name;

pub(crate) const CLUSTER_ISSUER: &str = "cert-manager.io/cluster-issuer";
pub(crate) const ISSUER: &str = "cert-manager.io/issuer";

/// A `ClusterIssuer` as the plan's reader found it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssuerRead {
    /// It exists. `wildcards`: it can issue a wildcard certificate (a DNS-01 solver, or not
    /// ACME).
    Found {
        /// Whether it can issue a wildcard certificate.
        wildcards: bool,
    },
    /// It does not exist.
    Missing,
    /// The person may not read it.
    Unreadable,
}

/// An own certificate the API checked: its key matches, it covers the hostname, and it has not
/// expired.
#[derive(Clone, PartialEq, Eq)]
pub struct CheckedCertificate {
    /// When it expires, RFC 3339.
    pub not_after: String,
    /// The certificate and its intermediates, PEM.
    pub certificate: String,
    /// The private key, PEM.
    pub key: String,
}

impl std::fmt::Debug for CheckedCertificate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CheckedCertificate")
            .field("not_after", &self.not_after)
            .finish_non_exhaustive()
    }
}

/// The Secret of the own certificate of `hostname` on Ingress `ingress`. The suffix differs
/// from the `-tls` Secrets cert-manager fills, so the two never share a Secret.
#[must_use]
pub fn own_secret_name(ingress: &str, hostname: &str) -> String {
    let own = route_name("", hostname);
    let mut name = if ingress.ends_with(&own) {
        ingress.to_owned()
    } else {
        route_name(ingress, hostname)
    };
    name.truncate(248);
    name.push_str("-cert");
    name
}

/// The Secret cert-manager fills for `hostname`: `<ingress>-tls` while no other entry names it.
pub(crate) fn issued_secret_name(ingress: &str, hostname: &str, tls: &[Value]) -> String {
    let first = format!("{ingress}-tls");
    let taken = tls.iter().any(|t| {
        t.get("secretName").and_then(Value::as_str) == Some(first.as_str())
            && !array(t, "/hosts").iter().all(|h| h == hostname)
    });
    if !taken {
        return first;
    }
    let mut name = route_name(ingress, hostname);
    name.truncate(249);
    name.push_str("-tls");
    name
}

fn secrets() -> ResourceRef {
    ResourceRef {
        group: String::new(),
        version: "v1".to_owned(),
        resource: "secrets".to_owned(),
    }
}

fn certificates() -> ResourceRef {
    ResourceRef {
        group: "cert-manager.io".to_owned(),
        version: "v1".to_owned(),
        resource: "certificates".to_owned(),
    }
}

/// The `tls` entry that lists `hostname`.
fn listed(tls: &[Value], hostname: &str) -> Option<Value> {
    tls.iter()
        .find(|t| array(t, "/hosts").iter().any(|h| h == hostname))
        .cloned()
}

fn secret_of(entry: &Value) -> Option<String> {
    entry
        .get("secretName")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// `tls` with `hostname` in an entry of its own that names `secret`.
fn with_own_entry(tls: &[Value], hostname: &str, secret: &str) -> Vec<Value> {
    let mut entries: Vec<Value> = tls
        .iter()
        .filter_map(|entry| {
            let hosts = array(entry, "/hosts");
            if !hosts.iter().any(|h| h == hostname) {
                return Some(entry.clone());
            }
            let rest: Vec<Value> = hosts.into_iter().filter(|h| h != hostname).collect();
            if rest.is_empty() {
                return None;
            }
            let mut entry = entry.clone();
            if let Some(map) = entry.as_object_mut() {
                map.insert("hosts".to_owned(), Value::Array(rest));
            }
            Some(entry)
        })
        .collect();
    entries.push(json!({"hosts": [hostname], "secretName": secret}));
    entries
}

/// Whether Ingress `object` names an issuer and routes hostnames other than `hostname`.
fn issuing_others(object: &Value, hostname: &str) -> bool {
    let named = [CLUSTER_ISSUER, ISSUER].iter().any(|key| {
        object
            .pointer(&pointer(&["metadata", "annotations", key]))
            .is_some()
    });
    named && rule_hosts(object).iter().any(|h| h != hostname)
}

/// Refuses to move `hostname` off Ingress `ingress` while the Ingress asks for a password, or
/// the hostname redirects or is in maintenance: the middlewares for these belong to the Ingress.
fn refuse_move(
    meta: &ObjectMeta,
    ingress: &str,
    hostname: &str,
    in_maintenance: bool,
) -> Result<(), PlanError> {
    let listed = traefik::middlewares(meta);
    let first = if listed.contains(&traefik::basic_auth_name(ingress)) {
        format!("Stop asking for a password on {ingress}")
    } else if listed.contains(&traefik::redirect_name(ingress, hostname)) {
        format!("Stop redirecting {hostname}")
    } else if in_maintenance {
        format!("End maintenance on {hostname}")
    } else {
        return Ok(());
    };
    Err(error(
        ErrorKind::Invalid,
        "certificate_shared",
        format!(
            "Ingress {ingress} asks cert-manager for the certificates of its other hostnames, so \
             {hostname} moves to an Ingress of its own for your certificate. {first} first."
        ),
    ))
}

/// Ingress `name` for `hostname` alone, from Ingress `from`: its rules for the hostname, its
/// class, labels and annotations, without the issuer and the
/// middlewares Zeabur made for `from`. `pages` is the new Ingress's own errors middleware.
fn moved_ingress(
    from: &Value,
    namespace: &str,
    name: &str,
    hostname: &str,
    pages: Option<&str>,
) -> Value {
    let meta = meta_of(from);
    let ingress = meta.name.as_deref().unwrap_or_default();
    let made = [
        traefik::reference(namespace, &traefik::error_pages_name(ingress)),
        format!("{namespace}-{ingress}-redirect-"),
    ];
    let mut annotations = meta.annotations.clone().unwrap_or_default();
    let mut middlewares: Vec<String> = annotations
        .get(traefik::MIDDLEWARES_ANNOTATION)
        .into_iter()
        .flat_map(|list| list.split(','))
        .map(str::trim)
        .filter(|entry| !entry.is_empty() && !made.iter().any(|m| entry.starts_with(m.as_str())))
        .map(str::to_owned)
        .collect();
    middlewares.extend(pages.map(|p| traefik::reference(namespace, p)));
    for key in [CLUSTER_ISSUER, ISSUER, LAST_APPLIED] {
        annotations.remove(key);
    }
    annotations.remove(traefik::MIDDLEWARES_ANNOTATION);
    if !middlewares.is_empty() {
        annotations.insert(
            traefik::MIDDLEWARES_ANNOTATION.to_owned(),
            middlewares.join(","),
        );
    }
    let rules: Vec<Value> = array(from, "/spec/rules")
        .into_iter()
        .filter(|r| r.get("host").and_then(Value::as_str) == Some(hostname))
        .collect();
    let mut body = json!({
        "apiVersion": "networking.k8s.io/v1",
        "kind": "Ingress",
        "metadata": {"name": name, "namespace": namespace},
        "spec": {"rules": rules},
    });
    let fields = [
        ("/metadata", "labels", meta.labels.map(|l| json!(l))),
        (
            "/metadata",
            "annotations",
            (!annotations.is_empty()).then(|| json!(annotations)),
        ),
        (
            "/spec",
            "ingressClassName",
            from.pointer("/spec/ingressClassName").cloned(),
        ),
    ];
    for (at, key, value) in fields {
        if let (Some(value), Some(map)) =
            (value, body.pointer_mut(at).and_then(Value::as_object_mut))
        {
            map.insert(key.to_owned(), value);
        }
    }
    body
}

/// The Secret the `tls` entry of `hostname` names.
fn listed_secret(object: &Value, hostname: &str) -> Option<String> {
    listed(&array(object, "/spec/tls"), hostname)
        .as_ref()
        .and_then(secret_of)
}

fn rule_hosts(object: &Value) -> Vec<String> {
    array(object, "/spec/rules")
        .iter()
        .filter_map(|r| r.get("host").and_then(Value::as_str).map(str::to_owned))
        .collect()
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "the planner's change kinds are split across modules by area"
)]
impl Planner<'_> {
    /// Fails unless `issuer`, which the person chose, exists and can issue for `hostname`.
    pub(crate) fn check_chosen_issuer(
        &self,
        issuer: &str,
        hostname: &str,
    ) -> Result<(), PlanError> {
        match self.input.issuers.get(issuer) {
            Some(IssuerRead::Found { wildcards }) => {
                if hostname.starts_with("*.") && !wildcards {
                    return Err(error(
                        ErrorKind::Invalid,
                        "wildcard_needs_dns",
                        format!(
                            "ClusterIssuer {issuer} has no DNS-01 solver, and Let's Encrypt \
                             issues a wildcard certificate such as {hostname} only through DNS-01."
                        ),
                    ));
                }
                Ok(())
            }
            Some(IssuerRead::Missing) => Err(error(
                ErrorKind::NotFound,
                "unknown_issuer",
                format!("There is no ClusterIssuer named {issuer}."),
            )),
            Some(IssuerRead::Unreadable) | None => Err(error(
                ErrorKind::Invalid,
                "issuer_unreadable",
                format!("Zeabur could not read ClusterIssuer {issuer} as you."),
            )),
        }
    }

    /// Whether `issuer`, which nobody chose in this request, can issue for `hostname`.
    pub(crate) fn issuer_fits(&self, issuer: &str, hostname: &str) -> bool {
        !hostname.starts_with("*.")
            || self.input.issuers.get(issuer) == Some(&IssuerRead::Found { wildcards: true })
    }

    /// The checked own certificate for `hostname`.
    pub(crate) fn checked(&self, hostname: &str) -> Result<CheckedCertificate, PlanError> {
        self.input
            .certificates
            .get(hostname)
            .cloned()
            .ok_or_else(|| {
                error(
                    ErrorKind::BadRequest,
                    "certificate_unchecked",
                    format!("The certificate for {hostname} was not checked."),
                )
            })
    }

    pub(crate) fn certificate(
        &mut self,
        route: &RouteRef,
        hostname: &str,
        source: &CertificateSource,
    ) -> Result<(), PlanError> {
        let hostname = hostname.to_ascii_lowercase();
        if route.kind == RouteKind::HttpRoute {
            return Err(error(
                ErrorKind::Invalid,
                "certificate_needs_ingress",
                format!(
                    "The certificate of HTTPRoute {} comes from its Gateway's listener, which \
                     belongs to the cluster; change it on the Gateway.",
                    route.name
                ),
            ));
        }
        let (object, meta, services) = self.route(route)?;
        Self::refuse_managed(&meta, "Ingress", &route.name)?;
        if !rule_hosts(&object).contains(&hostname) {
            return Err(error(
                ErrorKind::NotFound,
                "unknown_host",
                format!("{} does not route {hostname}.", route.name),
            ));
        }
        let behind = self.behind(&services);
        self.impact.extend(behind);
        let index = self.working(ingress_target(&route.name), object);
        match source {
            CertificateSource::Issuer { name } => {
                self.check_chosen_issuer(name, &hostname)?;
                self.use_issuer(index, &route.name, &hostname, name)
            }
            CertificateSource::Own { .. } if issuing_others(&self.current(index), &hostname) => {
                self.move_to_own(route, index, &hostname)
            }
            CertificateSource::Own { .. } => self.use_own(index, &route.name, &hostname),
        }
    }

    /// Moves `hostname` from Ingress `index`, which asks an issuer for its other hostnames too,
    /// to an Ingress of its own, and serves it there with the person's own certificate.
    fn move_to_own(
        &mut self,
        route: &RouteRef,
        index: usize,
        hostname: &str,
    ) -> Result<(), PlanError> {
        let current = self.current(index);
        let meta = meta_of(&current);
        let ingress = route.name.as_str();
        let in_maintenance = self.maintenance_ingress(ingress, hostname).is_some();
        refuse_move(&meta, ingress, hostname, in_maintenance)?;
        let services = array(&current, "/spec/rules")
            .iter()
            .filter(|r| r.get("host").and_then(Value::as_str) == Some(hostname))
            .flat_map(|r| array(r, "/http/paths"))
            .filter_map(|p| {
                p.pointer("/backend/service/name")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .collect();
        let name = self.own_route_name(&services, ingress, hostname)?;
        let pages = traefik::middlewares(&meta)
            .contains(&traefik::error_pages_name(ingress))
            .then(|| traefik::error_pages_name(&name));
        if let Some(pages) = &pages {
            let spec = errors_spec(&self.page_server()?);
            self.created_middleware(pages, None, &spec);
        }
        let namespace = self.input.snapshot.namespace.clone();
        let body = moved_ingress(&current, &namespace, &name, hostname, pages.as_deref());
        let issued = listed_secret(&current, hostname);
        self.remove_domain(route, hostname)?;
        if let Some(issued) = issued {
            self.delete_issued(&issued, ingress);
        }
        self.created(ingress_target(&name), body);
        self.warnings.push(Warning::DomainMoved {
            hostname: hostname.to_owned(),
            from: ObjectName::of("Ingress", ingress),
            to: ObjectName::of("Ingress", &name),
        });
        let moved = self.objects.len() - 1;
        self.use_own(moved, &name, hostname)
    }

    /// Makes Ingress `index` ask `issuer` for the certificate of `hostname`.
    pub(crate) fn use_issuer(
        &mut self,
        index: usize,
        ingress: &str,
        hostname: &str,
        issuer: &str,
    ) -> Result<(), PlanError> {
        let current = self.current(index);
        let annotations = current
            .pointer("/metadata/annotations")
            .and_then(Value::as_object)
            .cloned();
        let tls = array(&current, "/spec/tls");
        let own = own_secret_name(ingress, hostname);
        let mut operations = self.annotate(index, annotations.as_ref(), Some(issuer))?;
        let entry = listed(&tls, hostname);
        let replaced = entry.as_ref().and_then(secret_of).as_deref() == Some(own.as_str());
        if entry.is_none() || replaced {
            let entries =
                with_own_entry(&tls, hostname, &issued_secret_name(ingress, hostname, &tls));
            operations.extend(self.set_tls(index, &current, &entries)?);
        }
        let changed = !operations.is_empty();
        self.patch(index, operations)?;
        if replaced && self.input.snapshot.secret(&own).is_some() {
            self.delete(&secrets(), &own, None)?;
        }
        let others: Vec<String> = array(&current, "/spec/tls")
            .iter()
            .flat_map(|t| array(t, "/hosts"))
            .filter_map(|h| h.as_str().map(str::to_owned))
            .filter(|h| h != hostname)
            .collect();
        if changed && !others.is_empty() {
            self.warnings.push(Warning::IssuerShared {
                route: ingress.to_owned(),
                issuer: issuer.to_owned(),
                hostnames: others,
            });
        }
        self.warnings.push(Warning::HttpsCertificate {
            hostname: hostname.to_owned(),
            issuer: issuer.to_owned(),
        });
        Ok(())
    }

    /// Serves `hostname` on Ingress `index` with the person's own certificate.
    pub(crate) fn use_own(
        &mut self,
        index: usize,
        ingress: &str,
        hostname: &str,
    ) -> Result<(), PlanError> {
        let checked = self.checked(hostname)?;
        let current = self.current(index);
        let annotations = current
            .pointer("/metadata/annotations")
            .and_then(Value::as_object)
            .cloned();
        let named = annotations
            .as_ref()
            .is_some_and(|a| a.contains_key(CLUSTER_ISSUER) || a.contains_key(ISSUER));
        let others: Vec<String> = rule_hosts(&current)
            .into_iter()
            .filter(|h| h != hostname)
            .collect();
        if named && !others.is_empty() {
            return Err(error(
                ErrorKind::Invalid,
                "certificate_shared",
                format!(
                    "Ingress {ingress} asks cert-manager for the certificates of all its \
                     hostnames, also {}.",
                    others.join(", ")
                ),
            ));
        }
        let tls = array(&current, "/spec/tls");
        let secret = own_secret_name(ingress, hostname);
        let previous = listed(&tls, hostname).as_ref().and_then(secret_of);
        let mut operations = self.annotate(index, annotations.as_ref(), None)?;
        if previous.as_deref() != Some(secret.as_str()) {
            operations.extend(self.set_tls(
                index,
                &current,
                &with_own_entry(&tls, hostname, &secret),
            )?);
        }
        self.patch(index, operations)?;
        let owner = owner_of(&meta_of(&current));
        self.own_secret(&secret, owner.as_ref(), &checked)?;
        if let Some(previous) = previous.filter(|p| named && *p != secret) {
            self.delete_issued(&previous, ingress);
        }
        self.warnings.push(Warning::OwnCertificate {
            hostname: hostname.to_owned(),
            not_after: checked.not_after,
        });
        Ok(())
    }

    /// The operations that make the Ingress name `issuer` only, or no issuer for `None`.
    fn annotate(
        &mut self,
        index: usize,
        annotations: Option<&Map<String, Value>>,
        issuer: Option<&str>,
    ) -> Result<Vec<Value>, PlanError> {
        let mut operations = Vec::new();
        let now = |key: &str| annotations.and_then(|a| a.get(key)).cloned();
        for key in [CLUSTER_ISSUER, ISSUER] {
            let wanted = (key == CLUSTER_ISSUER).then_some(issuer).flatten();
            let before = now(key);
            if before.as_ref().and_then(Value::as_str) == wanted {
                continue;
            }
            self.claim(
                index,
                &[
                    Step::field("metadata"),
                    Step::field("annotations"),
                    Step::field(key),
                ],
                "annotations",
            )?;
            let path = pointer(&["metadata", "annotations", key]);
            match (before, wanted) {
                (Some(before), Some(wanted)) => {
                    operations.extend([test(&path, before), replace(&path, json!(wanted))]);
                }
                (Some(before), None) => operations.extend([test(&path, before), remove(&path)]),
                (None, Some(wanted)) if annotations.is_some() => {
                    operations.push(add(&path, json!(wanted)));
                }
                (None, Some(wanted)) => {
                    operations.push(add("/metadata/annotations", json!({ key: wanted })));
                }
                (None, None) => {}
            }
        }
        Ok(operations)
    }

    fn set_tls(
        &mut self,
        index: usize,
        current: &Value,
        entries: &[Value],
    ) -> Result<Vec<Value>, PlanError> {
        self.claim(index, &[Step::field("spec"), Step::field("tls")], "tls")?;
        Ok(current.pointer("/spec/tls").map_or_else(
            || vec![add("/spec/tls", json!(entries))],
            |before| {
                vec![
                    test("/spec/tls", before.clone()),
                    replace("/spec/tls", json!(entries)),
                ]
            },
        ))
    }

    /// Creates or updates the `kubernetes.io/tls` Secret `name`.
    fn own_secret(
        &mut self,
        name: &str,
        owner: Option<&Value>,
        checked: &CheckedCertificate,
    ) -> Result<(), PlanError> {
        let encode = |text: &str| base64::engine::general_purpose::STANDARD.encode(text);
        let data =
            json!({"tls.crt": encode(&checked.certificate), "tls.key": encode(&checked.key)});
        let Some(existing) = self.input.snapshot.secret(name) else {
            self.created(
                Target::of::<k8s_openapi::api::core::v1::Secret>(name),
                json!({
                    "apiVersion": "v1",
                    "kind": "Secret",
                    "metadata": self.owned_metadata(name, owner),
                    "type": "kubernetes.io/tls",
                    "data": data,
                }),
            );
            return Ok(());
        };
        if existing.type_.as_deref() != Some("kubernetes.io/tls") {
            return Err(error(
                ErrorKind::Conflict,
                "secret_not_tls",
                format!("Secret {name} exists and is not of type kubernetes.io/tls."),
            ));
        }
        let object = to_json(existing)?;
        let index = self.working(
            Target::of::<k8s_openapi::api::core::v1::Secret>(name),
            object,
        );
        self.patch(index, vec![add("/data", data)])
    }

    /// Deletes the `Certificate` cert-manager made for Secret `secret` of Ingress `ingress`, if
    /// it was read and the Ingress owns it.
    /// Deletes ingress-shim's Certificate for `secret`. The generic delete refuses it, as the
    /// Ingress owns it; the Ingress no longer names an issuer, so nothing makes it again.
    fn delete_issued(&mut self, secret: &str, ingress: &str) {
        let resource = certificates();
        let issued = self.input.objects.iter().find(|f| {
            f.resource == resource
                && f.name == secret
                && array(&f.object, "/metadata/ownerReferences")
                    .iter()
                    .any(|o| {
                        o.get("kind").and_then(Value::as_str) == Some("Ingress")
                            && o.get("name").and_then(Value::as_str) == Some(ingress)
                    })
        });
        let Some(issued) = issued else {
            return;
        };
        let target = Target {
            api_version: resource.api_version(),
            kind: "Certificate".to_owned(),
            plural: resource.resource.clone(),
            name: secret.to_owned(),
        };
        self.objects.push(Working {
            target,
            original: Some(issued.object.clone()),
            current: issued.object.clone(),
            operations: Vec::new(),
            owners: Vec::new(),
            deleted: true,
        });
    }
}
