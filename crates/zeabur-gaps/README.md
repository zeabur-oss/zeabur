# zeabur-gaps

Finds what a project is missing (gaps) and explains why a workload is unhealthy (diagnoses), as
stable sentence keys with their values, so every client words them in its own language.

## Diagnoses

`diagnose` reads one projected service (its pods' container statuses and its containers), the
namespace's snapshot (workload conditions, pod templates and scheduling, the time of the read),
the project's events and its `HTTPRoutes`, and returns what fails: a crash loop, a failed Job
run, out of memory, an image that cannot be pulled, a failing probe, traffic that cannot reach
it, a quota that blocks its pods, no node with room, a missing Secret or ConfigMap, and, for a
failing service nothing else explains, the reason the cluster gave. Each has a cause, why (the
rule that applies), a fix and the evidence (status fields and events) it rests on, and a fault:
where the cause lies (the code, the configuration, the cluster, a component, an upstream
registry) and who can remove it (you, a cluster administrator, the component's owner). See the
`diagnose` module for the rules and their sources.

## Incidents

`incidents` reads the cluster's components (an ingress controller, cert-manager, cluster DNS,
metrics-server, recognised by their labels) and nodes, and turns each one that is down into one
incident with its blast radius: the services, domains and projects it reaches. `link` marks the
diagnoses an incident explains, so a cluster's fault is not shown as each service's own. See the
`incident` module for what each cause reaches.

## Route traces

`trace` follows every way to a service, hop by hop: for each domain, DNS (answers the caller looked
up), the ingress controller's address and readiness, the certificate, the route's status; for
every address, the Service, its target port and the pods' readiness. Each hop is `ok`, `failed`
or `unknown` with what was expected and what was seen; `diagnoses` turns the first failed hop of
each address into a diagnosis. See the `trace` module for each hop's rule.

## Gap rules

A rule implements `Rule`: an id, the guidance it cites, and a check over the project and a few
facts about the cluster (`ClusterTraits`). `BUILTIN` holds the rules Zeabur always runs; a
module passes its own rules to `evaluate` next to them, so registration stays explicit and this
crate never depends on a module.

| Rule | Finds | Cites |
|---|---|---|
| `resource-limits` | a container without CPU and memory requests or a memory limit | Kubernetes resource management, Pod quality of service classes |
| `health-checks` | a long-running container behind a Service without a readiness probe | Kubernetes container probes |
| `ephemeral-data` | an official database image (postgres, mysql, mariadb, mongo, redis, valkey, rabbitmq) whose data directory is on no persistent volume | Kubernetes volumes and persistent volumes, each image's Dockerfile |
| `ingress-to-httproute` | an Ingress of a retired controller, or one that can move to Gateway API | Kubernetes Ingress, the ingress-nginx retirement notice, Gateway API's migration guide |

## Hides

The rule set and the sentences. Each rule cites the guidance it rests on and has a test; the
cases in `fixtures/gaps/` run against both.

## Place in the workspace

Level 3: may depend on `zeabur-kernel`, `zeabur-model` and `zeabur-graph`.
