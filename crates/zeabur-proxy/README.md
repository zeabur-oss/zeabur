# zeabur-proxy

Zeabur Proxy: lets a person read a shared backend's data for one namespace, as far as their own
Kubernetes RBAC rights reach. The first backend is VictoriaLogs (log history, `/logs/`). Zeabur
calls it with the person's token; browsers never reach it.

## A request

1. Only `Authorization: Bearer` is read; no cookie, no other header.
2. The path prefix picks the backend module (`/logs/`); none: 404.
3. The module parses the request once into a fixed query: method, path and parameters must be on
   its lists, `namespace` exactly once and a DNS label; otherwise 400.
4. A `SelfSubjectAccessReview` with the person's token asks for `get` on the resource (`logs`) in
   the group `proxy.zeabur.com` in that namespace. No: 403. kube-apiserver failing or slow: 503,
   never let through. Answers are reused for 10 s, keyed by a hash of the token, the namespace and
   the resource.
5. The namespace is read with the person's token for its `creationTimestamp`; not readable: 403.
6. The backend's request is rebuilt from the parsed query alone, with the namespace filter, the
   time range cut to start no earlier than the namespace's creation, and the module's limits. The
   original query string and headers are never sent on.
7. The answer is streamed back within a time and a size, with only `Content-Type` of its headers.
8. One audit line: user, namespace, backend, path, decision, duration and bytes; never the token,
   the query or the data.

Types hold this order: a `Namespace` only comes from its validator, a `Grant` only from the
access review, and a backend module builds its request only from an `Authorized` query, which only
a query and a grant for the same namespace and resource make. The rules (`rules`, `logs`) do no
I/O.

## Deployment

The Zeabur chart installs it with `proxy.enabled` (`deploy/chart/zeabur`). Its pod mounts no
service account token: the proxy can only ask as the person. Zeabur reaches it over TLS; the
chart makes a CA and a certificate, or takes a Secret (for example from cert-manager), and the
proxy reads a renewed certificate without a restart.

## Place in the workspace

Level 1. It runs as its own component, so it may depend on no workspace crate
(`[workspace.metadata.isolated]`, checked by `scripts/check-levels.sh`), and it
talks to kube-apiserver with kube-rs directly rather than through `zeabur-kube`, keeping what it
trusts small. The parser has a fuzz target in `fuzz/` (`cargo fuzz run parse`).

## References

- VictoriaLogs, "Querying": extra filters.
  <https://docs.victoriametrics.com/victorialogs/querying/#extra-filters>
- Kubernetes, "Authorization": checking API access.
  <https://kubernetes.io/docs/reference/access-authn-authz/authorization/#checking-api-access>
- Kubernetes, "Using RBAC Authorization": aggregated ClusterRoles.
  <https://kubernetes.io/docs/reference/access-authn-authz/rbac/#aggregated-clusterroles>
- observatorium/opa-openshift: LokiStack's authorization with `loki.grafana.com`.
  <https://github.com/observatorium/opa-openshift>
- prometheus-community/prom-label-proxy: `--error-on-replace`.
  <https://github.com/prometheus-community/prom-label-proxy>
