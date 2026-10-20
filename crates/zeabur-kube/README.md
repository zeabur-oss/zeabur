# zeabur-kube

The only crate that talks to kube-apiserver.

## Hides

How a request reaches the cluster. The port it exports reads and writes *as a given user*: the
caller hands over the user's credential (a bearer token issued by Zeabur Identity, or the local
kubeconfig context) and gets back a client bound to it. No exported function returns a client
built from Zeabur's own service account, so no layer above can act with more rights than the
person using it; kube-apiserver makes every authorisation decision.

The port ([`Cluster`](crate::port::Cluster)) has two adapters: kube-rs against a real cluster
([`UserClient`]), and an in-memory store for tests (`memory::MemoryCluster`, behind the `memory`
feature).

## Design

- **No cache.** Every call reads fresh, as the person. A cache shared between people could show
  one person what only another may see; a cache per person would keep answering after their
  rights were removed. The cluster test shows the first request after a RoleBinding is deleted
  is refused.
- **Partial rights, partial answers.** A namespace listing reads each kind separately; kinds
  the person may not list are reported as unreadable instead of failing the page.
- **Secrets without values.** Secrets are listed through kube-apiserver's Table rendering
  (`includeObject=Metadata`): name, type and key count. Only `reveal_secret` returns values; the
  generic reads below replace them (and the last-applied annotation that repeats them) before
  anything leaves this crate.
- **Any resource, discovery first.** Besides the typed reads, the port reads any served
  resource generically (`served_resources`, `list_objects`, `list_metadata`, `get_object`), for
  custom resources and integrations. Each asks discovery first, so an API that is not installed is
  reported as not installed, never as a missing permission.

## Place in the workspace

Level 1: may depend on `zeabur-core`.

## What kube-rs provides, checked against kube-apiserver

`tests/kubernetes.rs` runs on a disposable k3s cluster and checks, through a client that carries
only a test person's token:

- **JSON Patch** with the field manager `zeabur` [rfc6902]: the patch changes only the fields it
  names, another manager's fields stay, and `managedFields` names `zeabur` for what it changed.
  A `test` that no longer holds (a `resourceVersion` moved on) is answered `422 Invalid`, which
  the API reports as a conflict once it has read the object again.
- **The write contract** (`tests/contract/`): one suite of behaviours run against both k3s and
  the in-memory cluster (`tests/memory.rs`), so the fake the API tests use answers as
  kube-apiserver does: the object stored after a patch is the one RFC 6902 makes of it, the
  generation moves only with the spec, failed `test` operations, `409 AlreadyExists`, `404` for a
  patch of nothing, strict field validation [field-validation] in dry runs [dry-run] (which store
  nothing) and real writes, and one winner among concurrent patches of one version.
- **Identity**: `SelfSubjectReview` reports the person, and a namespace outside their rights
  answers `403`. That the first request after their `RoleBinding` is deleted answers `403` (Zeabur
  holds no cache of permissions that could outlive them) is checked through the API, in
  `zeabur-api`'s cluster tests.

- **Exec** over WebSocket [kep-4006] (kube's `ws` feature), behind the terminal: it runs as the
  person, and is refused until a RoleBinding grants `create` on `pods/exec`.

[dry-run]: https://kubernetes.io/docs/reference/using-api/api-concepts/#dry-run
[field-validation]: https://kubernetes.io/docs/reference/using-api/api-concepts/#field-validation
[rfc6902]: https://www.rfc-editor.org/rfc/rfc6902
[kep-4006]: https://github.com/kubernetes/enhancements/blob/master/keps/sig-api-machinery/4006-transition-spdy-to-websockets/README.md
