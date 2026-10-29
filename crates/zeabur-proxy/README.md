# zeabur-proxy

Zeabur Proxy: lets a person read a shared backend's data for one namespace, as far as their own
Kubernetes RBAC rights reach. The backends are VictoriaLogs (log history, `/logs/`) and
VictoriaMetrics (usage history, `/metrics/`). Zeabur calls it with the person's token; browsers
never reach it.

## A request

1. Only `Authorization: Bearer` is read; no cookie, no other header.
2. The path prefix picks the backend module (`/logs/`, `/metrics/`); none: 404.
3. The module parses the request once into a fixed query: method, path and parameters must be on
   its lists, `namespace` exactly once and a DNS label; otherwise 400.
4. A `SelfSubjectAccessReview` with the person's token asks for `get` on the resource (`logs`,
   `metrics`) in the group `proxy.zeabur.com` in that namespace. No: 403. kube-apiserver failing
   or slow: 503, never let through. Answers are reused for 10 s, keyed by a hash of the token, the
   namespace and the resource.
5. The namespace is read with the person's token for its `creationTimestamp`; not readable: 403.
6. The backend's request is rebuilt from the parsed query alone, with the namespace filter
   (VictoriaLogs' `extra_stream_filters`, VictoriaMetrics' `extra_label=namespace=<namespace>`),
   the time range cut so that no sample from before the namespace's creation is read, and the
   module's limits.
   The original query string and headers are never sent on. A filter parameter in the request is
   refused, not merged.
7. The answer is streamed back within a time and a size, with only `Content-Type` of its headers.
8. One audit line: user, namespace, backend, path, decision, duration and bytes; never the token,
   the query or the data.

Types hold this order: a `Namespace` only comes from its validator, a `Grant` only from the
access review, and a backend module builds its request only from an `Authorized` query, which only
a query and a grant for the same namespace and resource make. The rules (`rules`, `logs`,
`metrics`) do no I/O.

The metrics module serves the Prometheus query, range query, series, label names and label values
APIs, the ones VictoriaMetrics applies `extra_label` to. Export, federate, TSDB status, metadata
and the vmalert APIs are refused.

The namespace label keeps other namespaces out, but a namespace deleted and created again under
the same name keeps its label. Time keeps the earlier one out:

- A query reads samples before its first point: a range selector's window, a subquery's window
  and step, an `offset`, and for each selector and each subquery the step and 5 minutes more. The
  module bounds that reach from the query's text (`reach`): every duration in it, plus three steps
  and 5 minutes for each level a subquery may nest in (a subquery, or a function over a
  parenthesis or an operator), plus two steps, 5 minutes and 30 s. It moves the first point to
  after the creation by that much. A query whose reach it cannot bound is refused with 400: `@`,
  `WITH`, comments, and durations that are not whole literals (`[1i]`, `[$__interval]`,
  `[1.5h]`). The bound assumes VictoriaMetrics' `-search.minStalenessInterval` and
  `-search.latencyOffset` are at most their defaults (5 minutes, 30 s).
- The series and label APIs find series by VictoriaMetrics' per-day index, and a series active in
  the last hour of a day is also in the next day's. Their range starts at 00:00 UTC on the second
  day after the creation; before that they answer 400 `before_namespace`.

A query of a project created a few minutes ago therefore answers 400 `before_namespace`, and
Zeabur shows an empty history: for its first 8 minutes in the 1-hour view, 10 in the 6-hour view,
21 in the 24-hour view and 1 h 36 min in the 7-day view.

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
- VictoriaMetrics, "Prometheus querying API enhancements": `extra_label`.
  <https://docs.victoriametrics.com/victoriametrics/single-server-victoriametrics/#prometheus-querying-api-enhancements>
- Kubernetes, "Authorization": checking API access.
  <https://kubernetes.io/docs/reference/access-authn-authz/authorization/#checking-api-access>
- Kubernetes, "Using RBAC Authorization": aggregated ClusterRoles.
  <https://kubernetes.io/docs/reference/access-authn-authz/rbac/#aggregated-clusterroles>
- observatorium/opa-openshift: LokiStack's authorization with `loki.grafana.com`.
  <https://github.com/observatorium/opa-openshift>
- prometheus-community/prom-label-proxy: `--error-on-replace`.
  <https://github.com/prometheus-community/prom-label-proxy>
