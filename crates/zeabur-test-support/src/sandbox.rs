//! A namespace on a disposable cluster for one test, with a person in it: the cluster tests'
//! setup (`crates/*/tests/kubernetes.rs`), written once.
//!
//! The person is a ServiceAccount, [`PERSON`], with a Role of the test's rules bound to it and a
//! token from the `TokenRequest` API [k8s-token-request]; Zeabur reads as them through a
//! [`UserClient`]. The cluster admin (the current kubeconfig context) only prepares objects and
//! reads them back.
//!
//! [k8s-token-request]: https://kubernetes.io/docs/reference/kubernetes-api/authentication-resources/token-request-v1/

use std::sync::Mutex;

use kube::api::{Api, DeleteParams, PostParams};
use kube::{Client, Config};
use serde_json::{Value, json};
use zeabur_kube::k8s_openapi::api::authentication::v1::{TokenRequest, TokenRequestSpec};
use zeabur_kube::k8s_openapi::api::authorization::v1::{
    ResourceAttributes, SubjectAccessReview, SubjectAccessReviewSpec,
};
use zeabur_kube::k8s_openapi::api::core::v1::{Namespace, ServiceAccount};
use zeabur_kube::k8s_openapi::api::rbac::v1::{ClusterRole, ClusterRoleBinding, Role, RoleBinding};
use zeabur_kube::{ClusterEndpoint, UserClient, UserToken};

/// The name of the person's ServiceAccount, Role and RoleBinding.
pub const PERSON: &str = "alice";

/// A namespace with a person in it; dropping it deletes the namespace (see [`Cleanup`]).
pub struct Sandbox {
    admin: Client,
    endpoint: ClusterEndpoint,
    namespace: String,
    token: String,
    user: UserClient,
    cleanup: Cleanup,
}

impl std::fmt::Debug for Sandbox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sandbox")
            .field("namespace", &self.namespace)
            .finish_non_exhaustive()
    }
}

/// Waits until the RBAC authorizer allows the person the first of `rules`. It reads Roles and
/// RoleBindings from an informer cache, so a binding just created can take a moment to count, and a
/// test's first request would be refused meanwhile [k8s-rbac].
///
/// [k8s-rbac]: https://kubernetes.io/docs/reference/access-authn-authz/rbac/
#[expect(
    clippy::expect_used,
    clippy::panic,
    reason = "a test cannot go on without its sandbox"
)]
async fn bound(admin: &Client, namespace: &str, rules: &Value) {
    let first = |key: &str| {
        rules
            .get(0)
            .and_then(|rule| rule.get(key))
            .and_then(|values| values.get(0))
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    let (Some(verb), Some(group), Some(resource)) =
        (first("verbs"), first("apiGroups"), first("resources"))
    else {
        return;
    };
    let (resource, subresource) = match resource.split_once('/') {
        Some((resource, sub)) => (resource.to_owned(), Some(sub.to_owned())),
        None => (resource, None),
    };
    let review = SubjectAccessReview {
        spec: SubjectAccessReviewSpec {
            user: Some(format!("system:serviceaccount:{namespace}:{PERSON}")),
            resource_attributes: Some(ResourceAttributes {
                namespace: Some(namespace.to_owned()),
                verb: Some(verb),
                group: Some(group),
                resource: Some(resource),
                subresource,
                ..ResourceAttributes::default()
            }),
            ..SubjectAccessReviewSpec::default()
        },
        ..SubjectAccessReview::default()
    };
    let reviews = Api::<SubjectAccessReview>::all(admin.clone());
    for _ in 0..60 {
        let answer = reviews
            .create(&PostParams::default(), &review)
            .await
            .expect("subject access review");
        if answer.status.is_some_and(|s| s.allowed) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    panic!("the person's RoleBinding never took effect");
}

impl Sandbox {
    /// A new namespace whose person has a Role with `rules` (an array of RBAC `PolicyRule`s).
    ///
    /// # Panics
    ///
    /// When the cluster cannot be reached or refuses the admin; a test cannot go on without its
    /// sandbox.
    #[expect(
        clippy::expect_used,
        reason = "a test cannot go on without its sandbox"
    )]
    pub async fn new(prefix: &str, rules: Value) -> Self {
        let config = Config::infer().await.expect("kubeconfig");
        let endpoint = ClusterEndpoint::from_config(config.clone());
        let admin = Client::try_from(config).expect("admin client");
        let namespace = format!(
            "{prefix}-{}",
            zeabur_core::crypto::random_token::<4>().expect("random")
        );
        // Before anything is created, so a failure halfway is cleaned up too.
        let cleanup = Cleanup {
            namespace: namespace.clone(),
            cluster_roles: Mutex::new(Vec::new()),
        };
        create(
            &Api::<Namespace>::all(admin.clone()),
            json!({"metadata": {"name": namespace}}),
        )
        .await;
        let accounts = Api::<ServiceAccount>::namespaced(admin.clone(), &namespace);
        create(&accounts, json!({"metadata": {"name": PERSON}})).await;
        create(
            &Api::<Role>::namespaced(admin.clone(), &namespace),
            json!({"metadata": {"name": PERSON}, "rules": rules}),
        )
        .await;
        create(
            &Api::<RoleBinding>::namespaced(admin.clone(), &namespace),
            json!({"metadata": {"name": PERSON},
                   "roleRef": {"apiGroup": "rbac.authorization.k8s.io", "kind": "Role", "name": PERSON},
                   "subjects": [{"kind": "ServiceAccount", "name": PERSON, "namespace": namespace}]}),
        )
        .await;
        bound(&admin, &namespace, &rules).await;
        let request = TokenRequest {
            spec: TokenRequestSpec {
                audiences: vec![],
                expiration_seconds: Some(900),
                ..TokenRequestSpec::default()
            },
            ..TokenRequest::default()
        };
        let token = accounts
            .create_token_request(PERSON, &PostParams::default(), &request)
            .await
            .expect("token request")
            .status
            .expect("token")
            .token;
        let user = UserClient::with_token(&endpoint, &UserToken::new(token.clone())).expect("user");
        Self {
            admin,
            endpoint,
            namespace,
            token,
            user,
            cleanup,
        }
    }

    /// The cluster admin's connection, which only prepares objects and reads them back.
    #[must_use]
    pub const fn admin(&self) -> &Client {
        &self.admin
    }

    /// Where kube-apiserver is, without a credential.
    #[must_use]
    pub const fn endpoint(&self) -> &ClusterEndpoint {
        &self.endpoint
    }

    /// The namespace: the prefix and eight random hex digits, so runs never collide.
    #[must_use]
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// The person's token.
    #[must_use]
    pub fn token(&self) -> &str {
        &self.token
    }

    /// The person.
    #[must_use]
    pub const fn user(&self) -> &UserClient {
        &self.user
    }

    /// Deletes the `ClusterRole` and `ClusterRoleBinding` called `name` with the sandbox.
    pub fn delete_cluster_role(&self, name: &str) {
        if let Ok(mut names) = self.cleanup.cluster_roles.lock() {
            names.push(name.to_owned());
        }
    }

    /// An admin's handle on the namespace's objects of `K`.
    #[must_use]
    pub fn admin_api<K>(&self) -> Api<K>
    where
        K: kube::Resource<Scope = zeabur_kube::k8s_openapi::NamespaceResourceScope>,
        <K as kube::Resource>::DynamicType: Default,
    {
        Api::namespaced(self.admin.clone(), &self.namespace)
    }

    /// The person's handle on the namespace's objects of `K`, past Zeabur's port.
    #[must_use]
    pub fn user_api<K>(&self) -> Api<K>
    where
        K: kube::Resource<Scope = zeabur_kube::k8s_openapi::NamespaceResourceScope>,
        <K as kube::Resource>::DynamicType: Default,
    {
        Api::namespaced(self.user.kube().clone(), &self.namespace)
    }
}

/// Creates the object `value` through `api`.
///
/// # Panics
///
/// When kube-apiserver refuses it.
#[expect(
    clippy::expect_used,
    reason = "a test cannot go on without its objects"
)]
pub async fn create<K>(api: &Api<K>, value: Value)
where
    K: kube::Resource + Clone + serde::de::DeserializeOwned + serde::Serialize + std::fmt::Debug,
{
    api.create(
        &PostParams::default(),
        &serde_json::from_value(value).expect("an object of its kind"),
    )
    .await
    .expect("created");
}

/// Deletes a sandbox's namespace, and the cluster-scoped objects it names, when dropped: after
/// the test passes and while a failed assertion unwinds alike, so a failed run leaves nothing
/// behind for the next one.
///
/// Rust has no async drop, and `drop` runs on the test's runtime thread, where blocking on a
/// future panics and a spawned task may never run. So `drop` starts a thread with a runtime of
/// its own and a client of its own (a client's connection belongs to the runtime that made it),
/// and waits for the deletes to be sent. Failures are ignored: cleanup is best effort, and the
/// names are random, so a leftover never collides with a later run.
#[derive(Debug)]
struct Cleanup {
    namespace: String,
    cluster_roles: Mutex<Vec<String>>,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        let namespace = std::mem::take(&mut self.namespace);
        let cluster_roles = self
            .cluster_roles
            .get_mut()
            .map_or_else(|_| Vec::new(), std::mem::take);
        let cleanup = std::thread::spawn(move || {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };
            runtime.block_on(async move {
                let Ok(admin) = Client::try_default().await else {
                    return;
                };
                let delete = DeleteParams::background();
                for name in &cluster_roles {
                    let _ignored = Api::<ClusterRoleBinding>::all(admin.clone())
                        .delete(name, &delete)
                        .await;
                    let _ignored = Api::<ClusterRole>::all(admin.clone())
                        .delete(name, &delete)
                        .await;
                }
                let _ignored = Api::<Namespace>::all(admin)
                    .delete(&namespace, &delete)
                    .await;
            });
        });
        let _ignored = cleanup.join();
    }
}
