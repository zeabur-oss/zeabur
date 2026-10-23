//! Traefik `Middleware` objects on an Ingress [traefik-ingress]: the annotation that lists them,
//! and the names Zeabur gives the redirects and the basic auth it makes.
//!
//! [traefik-ingress]: https://doc.traefik.io/traefik/reference/routing-configuration/kubernetes/ingress/

use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

/// The Ingress annotation that lists the middlewares of its routers, in order.
pub const MIDDLEWARES_ANNOTATION: &str = "traefik.ingress.kubernetes.io/router.middlewares";

/// The longest object name.
const NAME_MAX: usize = 253;

/// How the annotation names `name` of `namespace`: `<namespace>-<name>@kubernetescrd`.
#[must_use]
pub fn reference(namespace: &str, name: &str) -> String {
    format!("{namespace}-{name}@kubernetescrd")
}

/// The names of the middlewares in `meta`'s namespace that the annotation lists, in order.
#[must_use]
pub fn middlewares(meta: &ObjectMeta) -> Vec<String> {
    let namespace = meta.namespace.as_deref().unwrap_or_default();
    meta.annotations
        .as_ref()
        .and_then(|a| a.get(MIDDLEWARES_ANNOTATION))
        .into_iter()
        .flat_map(|list| list.split(','))
        .filter_map(|entry| {
            entry
                .trim()
                .strip_suffix("@kubernetescrd")?
                .strip_prefix(namespace)?
                .strip_prefix('-')
                .map(str::to_owned)
        })
        .collect()
}

/// The middleware that redirects `hostname` of Ingress `ingress`.
#[must_use]
pub fn redirect_name(ingress: &str, hostname: &str) -> String {
    let mut name = format!("{ingress}-redirect-{}", hostname.replace('.', "-"));
    name.truncate(NAME_MAX);
    name.trim_end_matches('-').to_owned()
}

/// The middleware, and the Secret of its users, that ask for a password on Ingress `ingress`.
#[must_use]
pub fn basic_auth_name(ingress: &str) -> String {
    const SUFFIX: &str = "-basic-auth";
    let mut name = ingress.to_owned();
    name.truncate(NAME_MAX - SUFFIX.len());
    format!("{name}{SUFFIX}")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn only_this_namespaces_crd_middlewares_are_read() {
        let meta = ObjectMeta {
            namespace: Some("shop".to_owned()),
            annotations: Some(BTreeMap::from([(
                MIDDLEWARES_ANNOTATION.to_owned(),
                "shop-web-basic-auth@kubernetescrd, other-x@kubernetescrd,auth@file".to_owned(),
            )])),
            ..ObjectMeta::default()
        };
        assert_eq!(middlewares(&meta), vec!["web-basic-auth"]);
    }
}
