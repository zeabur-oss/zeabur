# Fixtures

Language-neutral test cases for the projection (`zeabur-graph`), the projected fields
(`zeabur-model`), change planning (`zeabur-plan`), diagnoses and gap rules (`zeabur-gaps`), and
the integrations (`zeabur-modules`). Each case is plain data: Kubernetes objects in, the expected
result out. Any implementation, in any language, can run them.

## Layout

Each crate keeps the cases it is tested with, so a packaged crate carries them:
`crates/zeabur-graph/fixtures/{graph,projection}`, `crates/zeabur-plan/fixtures/mutation`,
`crates/zeabur-gaps/fixtures/gaps` and `crates/zeabur-modules/fixtures/modules`. This directory
keeps the format, the operations and the chart corpus (`charts/`).

```
crates/<crate>/fixtures/<area>/<case>/
  case.yaml       what is tested
  input.yaml      Kubernetes objects, one YAML document each (optional)
  expected.json   the expected result
```

`<area>` is `graph`, `projection`, `mutation`, `gaps` or `modules`. `<case>` is a short kebab-case
name, unique in its area.

### `case.yaml`

```yaml
description: A Secret referenced by two Deployments is shared, not owned.
operation: graph.attribution     # one of the operations below
params:                          # optional: inputs that are not Kubernetes objects
  namespace: shop
```

### `input.yaml`

Complete Kubernetes objects (`apiVersion`, `kind`, `metadata.name`, and `metadata.namespace` for
namespaced kinds), separated by `---`. Only the fields a case depends on are filled in.

### `expected.json`

The result, in the JSON shape documented for the operation. A case that must fail expects
`{"error": "<code>"}`. Codes are stable identifiers, never message text, so implementations
may word their errors freely.

Comparison rules:

- A field present in `expected.json` is asserted; a field left out is not.
- A field set to `null` asserts that the field is absent from the result (or false for a
  boolean). An empty object `{}` asserts that the result is empty too.
- Arrays are compared in order, element by element, with the same rules. Where order carries no
  meaning, the operation defines one (below) so results are deterministic.
- Objects keyed by name compare as maps; key order is irrelevant.

## Operations

| Operation | Input | Params | Result |
|---|---|---|---|
| `graph.classify` | one workload | — | `{group, displayName: string \| null, managedBy: {kind, name, engine} \| null}`; database and operator workloads go to the groups `databases` and `operators` unless they carry an explicit group |
| `graph.resolveGroup` | — | `labels` (a string map; `{}` for none) | the group name, a string; `default` when nothing names one |
| `graph.attribution` | objects of one namespace | `namespace` | `{configMaps, secrets, persistentVolumeClaims, services}`, each a map name → `{ownership, owners}` (all four present, `{}` when empty), and `workloads: [{kind, name}]` sorted by kind then name |
| `graph.deploymentPhase` | one Deployment | — | one of `Running`, `Progressing`, `Degraded`, `Paused`, `Suspended`, `Completed`, `Failed`, `Unknown` |
| `graph.isMachinerySecret` | one Secret | — | a boolean |
| `projection.env` | one workload | `container` | `[EnvVar]`; a Secret-sourced value is masked and never present |
| `projection.envFrom` | a workload and the ConfigMaps/Secrets it imports | `container` | `[ImportedEnv]`; `keyCount` counts the imported object's keys |
| `projection.portProtocol` | one Service | `port` (the port's name) | `{protocol: HTTP \| HTTPS \| gRPC \| TCP \| UDP, inferredFrom: appProtocol \| name \| protocol}` |
| `projection.mounts` | a workload and its PVCs (for a StatefulSet, the PVCs created from its claim templates) | `container` | `[Mount]`, `category` one of `persistent`, `config`, `ephemeral` |
| `projection.provenance` | one object | — | `{tool: "Helm" \| "Argo CD" \| "Flux" \| null, managedBy, warnBeforeEdit}`; `managedBy` is the `app.kubernetes.io/managed-by` label verbatim |
| `projection.connections` | objects of one namespace | `namespace` | `[{from: {kind, name}, to: {kind, name}, inferred, evidence}]` sorted by `from` then `to`; `evidence` sorted, each `{basis: "sharedObject", object: {kind, name}}` (a non-data-store service and a data store read the same Secret or ConfigMap) or `{basis: "envValue", container, variable, service}` (a literal value names a Service of the namespace by short name or `<name>.<namespace>[.svc[.cluster.local]]`); `inferred` is true when every piece of evidence is `envValue` |
| `projection.pods` | one workload and its pods | — | `[PodSummary]` sorted by name; `reason` is the first waiting reason, else why a not-ready container last stopped; absent for a running, ready pod |
| `projection.containerResources` | one workload | `container` | `{requests: {cpu?, memory?}, limits: {cpu?, memory?}}` as written, an empty side left out; `null` when neither is set |
| `projection.routes` | one workload, its Services and Ingresses | — | `[Route]`; `tlsHosts` lists the hosts of `spec.tls`, sorted, absent when none |
| `modules.networkReachability` | Deployments (their pod template labels) and `NetworkPolicy` objects of one namespace | `namespace`; `connections: [[from, to]]` (Deployment names) | `{policies, services: [{service, selectedBy, ingressIsolated, egressIsolated, fromServices: [{service, access}], fromIngressController, fromOtherNamespaces, toDns, toOutside}], connections: [{from, to, access, blockedAt}]}`; services in input order, ids `projects/<namespace>/services/deployment/<name>`; `access` is `allowed`, `limited` (some ports, or labels zeabur cannot read) or `blocked`; `blockedAt` lists `egress` and/or `ingress` |
| `modules.backups` | PersistentVolumeClaims, CloudNativePG `Cluster`s, and K8up, Velero and CloudNativePG schedule and backup objects | `namespace` | `[Coverage]`: one row per claim (by name), then per database (`Cluster`, by name), then the namespace; each `{target: {kind: volume \| database \| namespace, name}, scheduledBy: ["<provider>:<namespace>/<name>"], lastRun?: {name, provider, outcome: succeeded \| partiallyFailed \| failed \| running \| unknown, started?, finished?, message?}}`; paused schedules do not count |
| `modules.backupGaps` | as `modules.backups` | `namespace`; `installed: {k8up?, velero?}` (bools) | `[{gap, object?}]`: one finding per claim and database no active schedule covers, in coverage order; a claim is `volume-without-backup` (advice) whose fix is a K8up `Schedule` (K8up installed), else a Velero `Schedule` (Velero installed), else installing K8up (`volume-backup/k8up`, no `object`); a database is `database-without-backup` (warning) with a CloudNativePG `ScheduledBackup` as `object` |
| `modules.networkGaps` | as `modules.networkReachability` | as `modules.networkReachability` | `[{gap, object?}]`: one `blocked-connection` warning per blocked connection, about the callee; blocked only at the callee's ingress, the fix is the plan `networkPolicy.allowFrom` (params `from`, `to`, and `object`, the allow-from-service preset as JSON) and no `object`; blocked at the caller's egress, the fix is the snippet `networkPolicy.allowEgress` with an egress NetworkPolicy from the caller's pods to the callee's as `object` |
| `mutation.validateEnv` | — | `changes: [{op: "set" \| "remove", name, value?, valueFrom?: {secretName?, configMapName?, key?, fieldPath?}}]` | `{ok: true}` or `{error}`: `no_changes`, `invalid_env_op`, `invalid_env_name`, `duplicate_env_name`, `ambiguous_env_source`, `missing_env_source_key` |
| `mutation.validateStorage` | the workload (its sole or named container) | `request: {name, size, mountPath, claimName?, accessMode?, container?}` | `{ok: true}` or `{error}`: `missing_volume_name`, `invalid_volume_name`, `invalid_claim_name`, `relative_mount_path`, `invalid_quantity`, `mount_path_in_use`, `mount_path_overlap` |
| `mutation.probeConfig` | the workload | `container`, `probe: {handler: "HTTPGet" \| "TCPSocket" \| "GRPC" \| "Exec", port?, path?, scheme?, command?}` | the Kubernetes `Probe`, or `{error}`: `missing_probe_port`, `invalid_probe_port`, `undeclared_probe_port`, `invalid_probe_path`, `invalid_probe_scheme`, `missing_probe_command`, `invalid_probe_timing` |
| `mutation.resourceAllowed` | — | `group`, `version`, `resource` | `{allowed: true}` or `{allowed: false, reason}`; reasons: `controller_managed`, `shared_infrastructure`, `tenant_level`, `privilege` |
| `mutation.reconcileIdentity` | — | `resource: {group, version, resource}`, `kind`, `namespace`, `name`, `body` (the raw apply document) | `{ok: true}` or `{error}`: `missing_api_version_or_kind`, `api_version_mismatch`, `kind_mismatch`, `name_mismatch`, `namespace_mismatch` |
| `mutation.ownedByStatefulSet` | — | `claim`, `statefulSet` | a boolean |
| `mutation.validateHostname` | — | `hostname` | `{ok: true}` or `{error}`: `hostname_required`, `invalid_hostname` |
| `mutation.routeName` | — | `workload`, `hostname` | the HTTPRoute name, a string |
| `mutation.diff` | before and after, in that order; a single object is a creation | — | `[{path, before?, after?}]` sorted by path; server-maintained fields and `status` omitted; Secret `data`/`stringData` values shown as `(hidden)` |
| `mutation.plan` | objects of one namespace (workloads, Pods, ReplicaSets, Services, ConfigMaps, Secrets without values, PersistentVolumeClaims, Ingresses, `HTTPRoute`s, and any object a YAML edit or a deletion names) | `request: {changes}` (`zeabur_plan::PlanRequest`); `reloader?` (bool: whether Stakater Reloader runs; absent: it could not be read); `gateways?: [{namespace, name}]` (`null`: they could not be listed; absent: none); `now?` (RFC 3339, default `2026-10-15T00:00:00Z`); `releases?: [[{kind, name}, [Release]]]` (for `rollback`); `storageClasses?: [{name, allowVolumeExpansion?, default?}]` (`null`: they could not be listed); `secretValues?: {name: {key: plain text}}` (the values the person could read); `reclaimPolicies?: {volume: policy}` (the PersistentVolumes the person could read, for `delete`); `kinds?: [{group, kind, plural, namespaced}]` (what discovery serves, for `create`; absent: only `CREATABLE`) | `{objects: [{target: {apiVersion, kind, plural, name}, before?, after? (absent for a deletion), changes: [{path, before?, after?}], owners: [{path, managers}], resourceVersion?, write}], impact: [{kind, name}], warnings: [{code, …}]}`; `write` is `{type: "patch", operations}` (RFC 6902, first testing `/metadata/resourceVersion`, then every value it replaces or removes) for an existing object, or `{type: "create", body}` (`POST`) for a new one, or `{type: "delete", uid?, resourceVersion}` (`DELETE`, background propagation, those preconditions); or `{error}`: `no_changes`, `unknown_workload`, `unknown_container`, `container_required`, `unknown_env_var`, `immutable_field`, `not_restartable`, `managed_by_helm`, `managed_by_argo_cd`, `managed_by_flux`, `field_managed_by_tool`, `unknown_object`, `invalid_key`, `key_in_binary_data`, `unknown_key`, `no_service`, `domain_exists`, `route_name_taken`, `route_matches_every_host`, `unknown_route`, `unknown_host`, `last_hostname`, `not_rollbackable`, `unknown_revision`, `already_current`, `unknown_claim`, `invalid_quantity`, `size_not_larger`, `expansion_not_allowed`, `stateful_set_volumes_fixed`, `volume_name_in_use`, `claim_name_taken`, `unknown_storage_class`, `unknown_mount`, `unknown_probe`, `not_scalable`, `invalid_replicas`, `not_reloadable`, `unchanged`, `unknown_build`, `invalid_path`, `unsupported_env_source`, `too_many_variables`, `unknown_resource`, `invalid_yaml`, `invalid_base64`, `binary_secret_value`, `other_namespace`, `owned_by_controller`, `changed_since`, `deleted_and_changed`, the reasons of `mutation.resourceAllowed`, and the codes of `mutation.validateEnv`, `mutation.validateHostname`, `mutation.validateStorage`, `mutation.probeConfig`, `mutation.reconcileIdentity` and `validate_resources` (`request_above_limit`) |
| `gaps.diagnose` | one workload, its pods (and a CronJob's Jobs), its Services, `HTTPRoutes` and Ingresses, `events.k8s.io/v1` Events, and other workloads that name it | `now` (RFC 3339; failures older than 10 minutes on a ready container do not count); `service?` (the workload diagnosed, when the input has several); `servicesReadable?` (bool, default `true`; `false`: the namespace's Services could not be listed, so the input has none) | `[Diagnosis]` sorted by kind then container: `{kind: crashLoop \| runFailed \| outOfMemory \| imagePull \| probeFailure \| unreachable \| quotaBlocked \| unschedulable \| configMissing \| unexplained, container?, pods, cause: {key, params}, why: {key, params}, fix: {key, params}, evidence: [Origin], lastSeen?, fault: {origin: code \| configuration \| cluster \| component \| upstream \| zeabur \| unknown, component?, fixer: you \| clusterAdmin \| componentOwner}}`; `container` is absent for what is about the service as a whole; an evidence `Origin` may carry the `value` read at its field; keys are stable sentence ids, never prose |
| `gaps.rules` | objects of one namespace, `HTTPRoute`s among them | `rules` (ids to run), `gatewayApi?` (bool), `retiredIngressClasses?` (strings), `gateways?: [{namespace, name}]` (the Gateways of every namespace; `null`: they could not be listed; absent: none), `servicesReadable?` (bool, default `true`; `false`: the namespace's Services could not be listed, so the input has none) | `[Gap]`, warnings first, then by rule and subject: `{rule, severity: warning \| advice, subject: Origin, service?, message: {key, params}, fix: {kind: plan \| install \| snippet, id, params}, citations: [{title, url}]}` |
| `gaps.snippets` | objects of one namespace | `gateways?: [{namespace, name}]` | every built-in rule's finding (Gateway API served) as `[{rule, fix, snippet}]`: the YAML fragment that fixes it, or `null` |

The value shapes (`EnvVar`, `ImportedEnv`, `Mount`, …) are those of `zeabur-model`. `EnvVar.source.kind`
is one of `Literal`, `Secret`, `ConfigMap`, `PodField`, `ResourceField`, `File` (`valueFrom.fileKeyRef`).
`ImportedEnv.keyCount` is absent when the count is unknown (a missing object, or a Secret whose
count was not read). Every projected item also has an `origin`: the object and field path where
it is declared, with keyed lists addressed by key
(`spec.template.spec.containers[name=app].env[name=X]`) and atomic lists by index (`envFrom[0]`).

Mutation operations use the field manager `zeabur`. A `mutation.diff` shows Secret values as
`(hidden)`.

One input cannot exist in a real cluster: `projection/mounts-dangling-mount-kept` has a
volumeMount without a volume, which kube-apiserver rejects. It stays because the projection must
never drop a mount silently, whatever it is given.

zeabur writes with one method, JSON Patch with `test` preconditions (and `POST` for a new
object), so the operator's server-side-apply rules (`mutation.ownedFields`,
`mutation.applyBody`) are not carried over.

## Running them

`zeabur-fixtures` loads a crate's cases (`load!("<area>")`) and compares results with them. The
crates under test run the cases of their area; `zeabur-test-support`'s test checks that every
case parses and names a known operation.
