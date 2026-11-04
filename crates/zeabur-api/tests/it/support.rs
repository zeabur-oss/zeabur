//! [`TestApi`]: the API over an in-memory cluster, with what a test changes from the defaults.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::http::request::Parts;
use secrecy::SecretString;
use zeabur_api::access::{ClusterAccess, Handoff, SingleCluster};
use zeabur_api::images::Inspector;
use zeabur_api::plan_token::PlanKey;
use zeabur_api::services::proxy::ZeaburProxy;
use zeabur_api::{Options, Settings};
use zeabur_core::problem::Problem;
use zeabur_kube::Cluster;
use zeabur_kube::k8s_openapi::jiff::{SignedDuration, Timestamp};

/// The API over one cluster, as one person, with [`Options::default`] but for each `with_*`.
pub(crate) struct TestApi {
    cluster: Arc<dyn Cluster>,
    token: Option<&'static str>,
    options: Options,
}

impl TestApi {
    pub(crate) fn new<C: Cluster + Clone + 'static>(cluster: &C) -> Self {
        Self {
            cluster: Arc::new(cluster.clone()),
            token: None,
            options: Options::default(),
        }
    }

    /// As a person signed in to the in-cluster server, whose `token`, expiring ten minutes after
    /// the clock, is handed to tools pods and Zeabur Proxy.
    pub(crate) const fn signed_in(mut self, token: &'static str) -> Self {
        self.token = Some(token);
        self
    }

    pub(crate) fn with_settings(mut self, settings: Settings) -> Self {
        self.options.settings = settings;
        self
    }

    pub(crate) fn with_clock(mut self, clock: fn() -> Timestamp) -> Self {
        self.options.clock = clock;
        self
    }

    /// Domains resolve to `answers` only.
    pub(crate) fn with_dns(mut self, answers: BTreeMap<String, Vec<String>>) -> Self {
        self.options.resolver = Arc::new(zeabur_api::resolve::Fixed(answers));
        self
    }

    /// `zeabur` is Zeabur's own service account in the terminal namespace.
    pub(crate) fn with_terminals<C: Cluster + Clone + 'static>(mut self, zeabur: &C) -> Self {
        self.options.terminals = Some(Arc::new(zeabur.clone()));
        self
    }

    pub(crate) fn with_zeabur_proxy(mut self, proxy: Arc<dyn ZeaburProxy>) -> Self {
        self.options.zeabur_proxy = Some(proxy);
        self
    }

    pub(crate) fn with_images(mut self, images: Inspector) -> Self {
        self.options.images = Some(Arc::new(images));
        self
    }

    pub(crate) const fn with_terminal_idle(mut self, idle: Duration) -> Self {
        self.options.terminal_idle = idle;
        self
    }

    pub(crate) fn with_plan_key(mut self, key: PlanKey) -> Self {
        self.options.plan_key = Some(Arc::new(key));
        self
    }

    /// The issuer's page for sign-in methods is `url`.
    pub(crate) fn with_sign_in(mut self, url: Option<&str>) -> Self {
        self.options.sign_in = Arc::new(zeabur_api::account::Fixed(url.map(str::to_owned)));
        self
    }

    pub(crate) fn router(self) -> Router {
        let access: Arc<dyn ClusterAccess> = match self.token {
            Some(token) => Arc::new(Signed {
                cluster: self.cluster,
                token,
                clock: self.options.clock,
            }),
            None => Arc::new(SingleCluster(self.cluster)),
        };
        zeabur_api::router_with(access, self.options)
    }
}

struct Signed {
    cluster: Arc<dyn Cluster>,
    token: &'static str,
    clock: fn() -> Timestamp,
}

impl ClusterAccess for Signed {
    fn for_request(&self, _parts: &Parts) -> Result<Arc<dyn Cluster>, Box<Problem>> {
        Ok(Arc::clone(&self.cluster))
    }

    fn handoff(&self, _parts: &Parts) -> Option<Handoff> {
        Some(Handoff {
            token: zeabur_kube::UserToken::new(SecretString::from(self.token)).unwrap(),
            expires_at: (self.clock)() + SignedDuration::from_mins(10),
        })
    }
}
