//! Following one object as the person: a LIST, then a watch from the list's `resourceVersion`
//! with bookmarks, and a new LIST when kube-apiserver answers `410 Gone` ("Efficient detection
//! of changes" [k8s-watch]).
//!
//! [k8s-watch]: https://kubernetes.io/docs/reference/using-api/api-concepts/#efficient-detection-of-changes

use std::time::Duration;

use futures_util::StreamExt as _;
use futures_util::stream::{self, BoxStream};
use kube::api::{Api, DynamicObject, ListParams, WatchEvent, WatchParams};
use tokio::time::Instant;

use crate::objects::ResourceRef;
use crate::port::{ClusterError, ObjectStates};
use crate::secrets;

/// The longest `timeoutSeconds` kube-rs sends: its connection's read timeout is 295 s.
const LONGEST_WATCH: u64 = 290;

/// `410 Gone`: the `resourceVersion` is older than kube-apiserver's watch history.
const GONE: u16 = 410;

type Events = BoxStream<'static, Result<WatchEvent<DynamicObject>, kube::Error>>;

struct Following {
    api: Api<DynamicObject>,
    secret: bool,
    selector: String,
    deadline: Instant,
    /// The `resourceVersion` to watch from; `None` until a LIST gives one.
    version: Option<String>,
    events: Option<Events>,
    done: bool,
}

/// What one request or event did to the stream.
#[expect(
    clippy::large_enum_variant,
    reason = "one step at a time, never stored"
)]
enum Step {
    /// A state to yield.
    State(Result<Option<DynamicObject>, ClusterError>),
    /// Nothing to yield: a bookmark, or a watch that started.
    Skip,
    /// The `resourceVersion` is gone: list again.
    Relist,
    /// The watch ended: watch again from the last `resourceVersion`.
    Rewatch,
}

/// The object `name` of `resource` in `namespace`, as [`crate::Cluster::watch_object`] says.
pub(crate) fn object(
    client: &kube::Client,
    resource: &ResourceRef,
    namespace: Option<&str>,
    name: &str,
    timeout: Duration,
) -> ObjectStates<'static> {
    let api_resource = resource.api_resource();
    let api = namespace.map_or_else(
        || Api::all_with(client.clone(), &api_resource),
        |namespace| Api::namespaced_with(client.clone(), namespace, &api_resource),
    );
    let following = Following {
        api,
        secret: resource.is_secret(),
        selector: format!("metadata.name={name}"),
        deadline: Instant::now() + timeout,
        version: None,
        events: None,
        done: false,
    };
    stream::unfold(following, |mut following| async move {
        let state = following.next().await?;
        Some((state, following))
    })
    .boxed()
}

impl Following {
    async fn next(&mut self) -> Option<Result<Option<DynamicObject>, ClusterError>> {
        loop {
            if self.done {
                return None;
            }
            // The deadline bounds every request, a LIST or a watch that stalls before it answers
            // included: `timeoutSeconds` bounds only a watch that has started.
            let step = tokio::time::timeout_at(self.deadline, self.step())
                .await
                .ok()??;
            match step {
                Step::State(state) => {
                    self.done = state.is_err();
                    return Some(state);
                }
                Step::Skip => {}
                Step::Relist => {
                    self.version = None;
                    self.events = None;
                }
                Step::Rewatch => self.events = None,
            }
        }
    }

    /// The next request or event; `None` once the deadline has passed.
    async fn step(&mut self) -> Option<Step> {
        if self.version.is_none() {
            return Some(self.list().await);
        }
        if let Some(events) = self.events.as_mut() {
            let event = events.next().await;
            return Some(event.map_or(Step::Rewatch, |event| self.event(event)));
        }
        self.watch().await
    }

    async fn list(&mut self) -> Step {
        let listed = self
            .api
            .list(&ListParams::default().fields(&self.selector))
            .await;
        match listed {
            Ok(list) => {
                self.version = Some(list.metadata.resource_version.unwrap_or_default());
                Step::State(
                    list.items
                        .into_iter()
                        .next()
                        .map(|o| self.hidden(o))
                        .transpose(),
                )
            }
            Err(error) => Step::State(Err(error.into())),
        }
    }

    /// Starts a watch from [`Self::version`] that kube-apiserver ends at the deadline; `None`
    /// once the deadline has passed.
    async fn watch(&mut self) -> Option<Step> {
        let left = self.deadline.checked_duration_since(Instant::now())?;
        if left.is_zero() {
            return None;
        }
        let seconds = left.as_secs().saturating_add(1).min(LONGEST_WATCH);
        let params = WatchParams::default()
            .fields(&self.selector)
            .timeout(u32::try_from(seconds).unwrap_or(u32::MAX));
        let version = self.version.clone().unwrap_or_default();
        Some(match self.api.watch(&params, &version).await {
            Ok(events) => {
                self.events = Some(events.boxed());
                Step::Skip
            }
            Err(kube::Error::Api(status)) if status.code == GONE => Step::Relist,
            Err(error) => Step::State(Err(error.into())),
        })
    }

    fn event(&mut self, event: Result<WatchEvent<DynamicObject>, kube::Error>) -> Step {
        match event {
            Ok(WatchEvent::Added(object) | WatchEvent::Modified(object)) => {
                self.version.clone_from(&object.metadata.resource_version);
                Step::State(self.hidden(object).map(Some))
            }
            Ok(WatchEvent::Deleted(object)) => {
                self.version.clone_from(&object.metadata.resource_version);
                Step::State(Ok(None))
            }
            Ok(WatchEvent::Bookmark(bookmark)) => {
                self.version = Some(bookmark.metadata.resource_version);
                Step::Skip
            }
            Ok(WatchEvent::Error(status)) if status.code == GONE => Step::Relist,
            Ok(WatchEvent::Error(status)) => Step::State(Err(kube::Error::Api(status).into())),
            Err(error) => Step::State(Err(error.into())),
        }
    }

    /// `object` without a Secret's values ([`crate::objects`]).
    fn hidden(&self, object: DynamicObject) -> Result<DynamicObject, ClusterError> {
        if !self.secret {
            return Ok(object);
        }
        let body = serde_json::to_vec(&object).map_err(|e| ClusterError::Decode(e.to_string()))?;
        secrets::one(&body)
    }
}
