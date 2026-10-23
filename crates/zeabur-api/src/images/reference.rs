//! An image reference as a container runtime reads it [distribution-reference]: a registry host
//! when the first segment has a dot, a port or is `localhost`, else Docker Hub, where a bare name
//! is under `library/`; then `:tag` or `@digest`, `latest` without either.
//!
//! [distribution-reference]: https://github.com/distribution/reference/blob/main/reference.go

/// Docker Hub, as credentials name it.
pub(crate) const DOCKER_HUB: &str = "docker.io";
/// Where Docker Hub's registry API answers.
const DOCKER_HUB_API: &str = "registry-1.docker.io";

/// A parsed image reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Reference {
    /// The registry as credentials name it: `docker.io`, `ghcr.io`, `registry.local:5000`.
    pub(crate) registry: String,
    /// The repository: `library/nginx`.
    pub(crate) repository: String,
    /// A tag or a `sha256:…` digest.
    pub(crate) tag: String,
}

impl Reference {
    /// `image`, or `None` when a container runtime would not accept it.
    pub(crate) fn parse(image: &str) -> Option<Self> {
        let image = image.trim();
        let (rest, digest) = match image.split_once('@') {
            Some((rest, digest)) => (rest, Some(digest)),
            None => (image, None),
        };
        let (name, tag) = match rest.rsplit_once(':') {
            Some((name, tag)) if !tag.contains('/') => (name, Some(tag)),
            _ => (rest, None),
        };
        let (registry, path) = match name.split_once('/') {
            Some((first, path))
                if first.contains('.') || first.contains(':') || first == "localhost" =>
            {
                (first.to_ascii_lowercase(), path.to_owned())
            }
            _ => (DOCKER_HUB.to_owned(), name.to_owned()),
        };
        let registry = registry_name(&registry);
        let repository = if registry == DOCKER_HUB && !path.contains('/') {
            format!("library/{path}")
        } else {
            path
        };
        if !valid_repository(&repository)
            || !valid_host(&registry)
            || tag.is_some_and(|t| !valid_tag(t))
            || digest.is_some_and(|d| !valid_digest(d))
        {
            return None;
        }
        Some(Self {
            registry,
            repository,
            tag: digest.or(tag).unwrap_or("latest").to_owned(),
        })
    }

    /// The host the registry API answers on.
    pub(crate) fn api_host(&self) -> &str {
        if self.registry == DOCKER_HUB {
            DOCKER_HUB_API
        } else {
            &self.registry
        }
    }
}

/// A registry named in a reference or a credential (`https://index.docker.io/v1/`,
/// `ghcr.io/acme`), as [`Reference::registry`] names it.
pub(crate) fn registry_name(server: &str) -> String {
    let server = server.trim().to_ascii_lowercase();
    let server = server
        .strip_prefix("https://")
        .or_else(|| server.strip_prefix("http://"))
        .unwrap_or(&server);
    let host = server.split('/').next().unwrap_or_default();
    match host {
        "docker.io" | "index.docker.io" | "registry-1.docker.io" | "registry.hub.docker.com" => {
            DOCKER_HUB.to_owned()
        }
        other => other.to_owned(),
    }
}

fn valid_host(host: &str) -> bool {
    !host.is_empty()
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b':' | b'[' | b']'))
}

fn valid_repository(repository: &str) -> bool {
    !repository.is_empty()
        && repository.split('/').all(|part| {
            !part.is_empty()
                && part.bytes().all(|b| {
                    b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-')
                })
                && part
                    .bytes()
                    .next()
                    .is_some_and(|b| b.is_ascii_alphanumeric())
        })
}

fn valid_tag(tag: &str) -> bool {
    (1..=128).contains(&tag.len())
        && tag
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_')
        && tag
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

fn valid_digest(digest: &str) -> bool {
    digest.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(image: &str) -> (String, String, String) {
        let r = Reference::parse(image).unwrap();
        (r.registry, r.repository, r.tag)
    }

    fn owned(a: &str, b: &str, c: &str) -> (String, String, String) {
        (a.to_owned(), b.to_owned(), c.to_owned())
    }

    #[test]
    fn references_are_read_as_a_container_runtime_reads_them() {
        assert_eq!(
            parts("nginx"),
            owned("docker.io", "library/nginx", "latest")
        );
        assert_eq!(
            parts("nginx:1.29"),
            owned("docker.io", "library/nginx", "1.29")
        );
        assert_eq!(
            parts("bitnami/redis:7"),
            owned("docker.io", "bitnami/redis", "7")
        );
        assert_eq!(
            parts("docker.io/library/nginx"),
            owned("docker.io", "library/nginx", "latest")
        );
        assert_eq!(
            parts("ghcr.io/acme/web:1.4"),
            owned("ghcr.io", "acme/web", "1.4")
        );
        assert_eq!(
            parts("registry.local:5000/app"),
            owned("registry.local:5000", "app", "latest")
        );
        assert_eq!(parts("localhost/app:dev"), owned("localhost", "app", "dev"));
        let digest = format!("sha256:{}", "a".repeat(64));
        assert_eq!(
            parts(&format!("quay.io/org/app:1@{digest}")),
            owned("quay.io", "org/app", &digest)
        );
    }

    #[test]
    fn what_a_runtime_refuses_is_not_a_reference() {
        for image in [
            "",
            "Nginx",
            "nginx:",
            "nginx@sha256:short",
            "ghcr.io/",
            "a//b",
            "nginx:bad tag",
        ] {
            assert_eq!(Reference::parse(image), None, "{image}");
        }
    }

    #[test]
    fn docker_hub_has_one_name_and_its_own_api_host() {
        for server in [
            "https://index.docker.io/v1/",
            "docker.io",
            "registry-1.docker.io",
        ] {
            assert_eq!(registry_name(server), "docker.io");
        }
        assert_eq!(registry_name("https://ghcr.io/acme"), "ghcr.io");
        assert_eq!(
            Reference::parse("nginx").map(|r| r.api_host().to_owned()),
            Some("registry-1.docker.io".to_owned())
        );
    }
}
