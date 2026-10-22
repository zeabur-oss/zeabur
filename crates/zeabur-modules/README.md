# zeabur-modules

Integrations with components a cluster may have installed: what Zeabur reads from each and what
it makes of it. One module per provider (metrics-server, system-upgrade-controller, …).

## Hides

Each component's resources and their shapes. The rest of Zeabur sees what a module declares
(its id, the capability it provides, the resources it reads) and the typed views it produces.

## Place in the workspace

Level 3: may depend on `zeabur-core`, `zeabur-model`, `zeabur-kube` (for the object types it
reads) and `zeabur-adapters` (for label selectors). Only the crates that compose Zeabur
(`zeabur-api`, `zeabur-mcp` and the binaries) may depend on it, never the core crates;
`scripts/check-levels.sh` checks both (`[workspace.metadata.modules]` in the root `Cargo.toml`).

## Design

- **An explicit list, compiled in.** [`modules`] returns every module, in a fixed order. Each is
  always compiled in: a component that is not installed is a run-time answer
  (`Reading::NotInstalled`), not a build option. There is no registration through linker
  sections (`inventory`, `linkme`): they rely on the linker keeping otherwise unreferenced
  statics and register nothing, silently, on platforms they do not support [inventory]. There is
  no loading at run time either: Zeabur is one static binary. Bevy composes plugins the same way,
  by a list in code [bevy-plugins].
- **Modules do not read.** A module declares the resources it needs ([`Module::reads`]), or the
  HTTP API it reads from a Service ([`Module::http`]: the metrics store); the composer reads them
  as the person, discovery (or the API's probe) first, so "not installed" and "not allowed" are
  told apart in one place ([`Read`]). The module turns the objects into its typed view with pure
  functions, which tests drive with plain JSON.
- **Schema names carry the provider.** A module's API types are named after it
  (`SystemUpgradeControllerPlan`), because the OpenAPI description gathers schemas by name and a
  second type with a taken name silently replaces the first; `zeabur-api`'s tests compare every
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
| `falco` | runtime-security | `wgpolicyk8s.io/v1alpha2` policyreports, clusterpolicyreports (written by falcosidekick) |
| `reloader` | config-reload | `apps/v1` deployments labelled `app.kubernetes.io/name: reloader`, cluster-wide |
| `barman-cloud` | postgres | `barmancloud.cnpg.io/v1` objectstores |
| `percona-server-mongodb` | mongodb | `psmdb.percona.com/v1` perconaservermongodbs, perconaservermongodbbackups |
| `zeabur-builder` | builds | `builder.zeabur.com/v1alpha1` builds, buildruns |

The databases capability (`databases`) has two providers, CloudNativePG and the Percona
Operator for MongoDB; like backups, they share only what a person asks of any database: whether
it is up, its members, version, where to connect, and its backups.

[inventory]: https://github.com/dtolnay/inventory
[bevy-plugins]: https://docs.rs/bevy/latest/bevy/app/trait.Plugin.html
