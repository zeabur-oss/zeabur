# engawa-modules

Integrations with components a cluster may have installed: what engawa reads from each and what
it makes of it. One module per provider (metrics-server, system-upgrade-controller, …).

## Hides

Each component's resources and their shapes. The rest of engawa sees what a module declares
(its id, the capability it provides, the resources it reads) and the typed views it produces.

## Place in the workspace

Level 2: may depend on `engawa-core`, `engawa-model`, `engawa-kube` (for the object types it
reads) and `engawa-adapters` (for label selectors). Only the crates that compose engawa
(`engawa-api`, `engawa-mcp` and the binaries) may depend on it, never the core crates;
`scripts/check-levels.sh` checks both (`[workspace.metadata.modules]` in the root `Cargo.toml`).

## Design

- **An explicit list, compiled in.** [`modules`] returns every module, in a fixed order. Each is
  always compiled in: a component that is not installed is a run-time answer
  (`Reading::NotInstalled`), not a build option. There is no registration through linker
  sections (`inventory`, `linkme`): they rely on the linker keeping otherwise unreferenced
  statics and register nothing, silently, on platforms they do not support [inventory]. There is
  no loading at run time either: engawa is one static binary. Bevy composes plugins the same way,
  by a list in code [bevy-plugins].
- **Modules do not read.** A module declares the resources it needs ([`Module::reads`]), or the
  HTTP API it reads from a Service ([`Module::http`]: the metrics store); the composer reads them
  as the person, discovery (or the API's probe) first, so "not installed" and "not allowed" are
  told apart in one place ([`Read`]). The module turns the objects into its typed view with pure
  functions, which tests drive with plain JSON.
- **Schema names carry the provider.** A module's API types are named after it
  (`SystemUpgradeControllerPlan`), because the OpenAPI description gathers schemas by name and a
  second type with a taken name silently replaces the first; `engawa-api`'s tests compare every
  answer type's schemas with the description.
- **A shared model only where several providers exist.** The backups capability has three
  providers (K8up, Velero, CloudNativePG); they share what is common to all of them (targets,
  schedules, run outcomes) and nothing more. Restoring stays with each provider. A capability
  with one provider (NetworkPolicy reachability, resource metrics, metrics history, node
  updates) is its module alone.

## Modules

| Module | Capability | Reads |
|---|---|---|
| `metrics-server` | metrics | `metrics.k8s.io/v1beta1` nodes, pods |
| `prometheus-api` | metrics | the Prometheus HTTP API of the metrics store's Service, through the service proxy |
| `system-upgrade-controller` | node-updates | `upgrade.cattle.io/v1` plans |
| `network-policy` | network-policy | `networking.k8s.io/v1` networkpolicies |
| `k8up` | volume-backup | `k8up.io/v1` schedules, backups |
| `velero` | volume-backup | `velero.io/v1` schedules, backups |
| `cloudnative-pg` | postgres | `postgresql.cnpg.io/v1` clusters, scheduledbackups, backups |

[inventory]: https://github.com/dtolnay/inventory
[bevy-plugins]: https://docs.rs/bevy/latest/bevy/app/trait.Plugin.html
