//! The page server a project runs for its error pages and maintenance page, and the Ingress that
//! shows the maintenance page for a hostname.
//!
//! The page server is what Zeabur Catalog's `error-pages` template deploys. Zeabur finds it by
//! [`PAGE_SERVER_LABEL`], not by name: its Service has the ports [`PAGES_PORT`] and
//! [`MAINTENANCE_PORT`], and its ConfigMap holds the pages by file name, such as `502.html`.

use k8s_openapi::api::core::v1::Service;
use k8s_openapi::api::networking::v1::Ingress;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

/// The label, set to `"true"`, on the Service and the ConfigMap of the page server.
pub const PAGE_SERVER_LABEL: &str = "zeabur.com/page-server";

/// The name of the Service port that serves each page by its file name, such as `/502.html`.
pub const PAGES_PORT: &str = "pages";

/// The name of the Service port that answers every request with `maintenance.html` and status
/// 503.
pub const MAINTENANCE_PORT: &str = "maintenance";

/// A page server's Service: its name and the numbers of its two ports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageServer {
    /// The Service's name.
    pub name: String,
    /// The number of its [`PAGES_PORT`].
    pub pages: i32,
    /// The number of its [`MAINTENANCE_PORT`].
    pub maintenance: i32,
}

/// Whether the object carries [`PAGE_SERVER_LABEL`].
#[must_use]
pub fn is_page_server(meta: &ObjectMeta) -> bool {
    crate::adapters::label(meta.labels.as_ref(), PAGE_SERVER_LABEL) == Some("true")
}

/// The page server `service` is: `None` without the label or without both ports.
#[must_use]
pub fn page_server(service: &Service) -> Option<PageServer> {
    if !is_page_server(&service.metadata) {
        return None;
    }
    let port = |name: &str| {
        service
            .spec
            .as_ref()?
            .ports
            .iter()
            .flatten()
            .find(|p| p.name.as_deref() == Some(name))
            .map(|p| p.port)
    };
    Some(PageServer {
        name: service.metadata.name.clone()?,
        pages: port(PAGES_PORT)?,
        maintenance: port(MAINTENANCE_PORT)?,
    })
}

/// The Ingress that shows the maintenance page for some hostnames of another Ingress: the name of
/// that Ingress and the hostnames. `None` for any other Ingress.
///
/// Such an Ingress has an owner reference to the other Ingress, and every path sends to the Service
/// port named [`MAINTENANCE_PORT`].
#[must_use]
pub fn maintenance_of(ingress: &Ingress) -> Option<(&str, Vec<String>)> {
    let owner = ingress
        .metadata
        .owner_references
        .iter()
        .flatten()
        .find(|o| o.kind == "Ingress" && o.api_version == "networking.k8s.io/v1")?;
    let rules = ingress.spec.as_ref()?.rules.as_deref()?;
    let paths: Vec<_> = rules
        .iter()
        .flat_map(|r| r.http.iter().flat_map(|h| &h.paths))
        .collect();
    let shows_page = !paths.is_empty()
        && paths.iter().all(|p| {
            p.backend
                .service
                .as_ref()
                .and_then(|s| s.port.as_ref())
                .and_then(|port| port.name.as_deref())
                == Some(MAINTENANCE_PORT)
        });
    shows_page.then(|| {
        let hosts = rules.iter().filter_map(|r| r.host.clone()).collect();
        (owner.name.as_str(), hosts)
    })
}
