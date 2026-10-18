# engawa-gaps

Finds what a project is missing (gaps) and explains why a workload is unhealthy (diagnoses), as
stable sentence keys with their values, so every client words them in its own language.

## Diagnoses

`diagnose` reads one projected service (its pods' container statuses and its containers), the
namespace's snapshot (workload conditions, pod templates and scheduling, the time of the read),
the project's events and its `HTTPRoutes`, and returns what fails: a crash loop, a failed Job
run, out of memory, an image that cannot be pulled, a failing probe, traffic that cannot reach
it, a quota that blocks its pods, no node with room, a missing Secret or ConfigMap, and, for a
failing service nothing else explains, the reason the cluster gave. Each has a cause, why (the
rule that applies), a fix and the evidence (status fields and events) it rests on. See the
`diagnose` module for the rules and their sources.

## Gap rules

A rule implements `Rule`: an id, the guidance it cites, and a check over the project and a few
facts about the cluster (`ClusterTraits`). `BUILTIN` holds the rules engawa always runs; a
module passes its own rules to `evaluate` next to them, so registration stays explicit and this
crate never depends on a module.

| Rule | Finds | Cites |
|---|---|---|
| `resource-limits` | a container without CPU and memory requests or a memory limit | Kubernetes resource management, Pod quality of service classes |
| `health-checks` | a long-running container behind a Service without a readiness probe | Kubernetes container probes |
| `ingress-to-httproute` | an Ingress of a retired controller, or one that can move to Gateway API | Kubernetes Ingress, the ingress-nginx retirement notice, Gateway API's migration guide |

## Hides

The rule set and the sentences. Each rule cites the guidance it rests on and has a test; the
cases in `fixtures/gaps/` run against both.

## Place in the workspace

Level 3: may depend on `engawa-core`, `engawa-model` and `engawa-graph`.
