# CloudNativePG image updates

The database form reuses the service policy, schedule controls and four Keel annotations.
Only same-tag refreshes are supported: a PostgreSQL major-version rolling tag may acquire a new
digest, but selecting another major or image flavor is a database migration. Catalog-managed
images, unversioned tags, externally managed Clusters and approvals are not supported.

## Components and permissions

`updater/` builds `zeabur-database-updater`, a separate Go process. It imports the unmodified
Keel 0.22.4 registry reader and poller through the pinned module in `go.mod`. It requires neither
a Keel fork nor changes to the catalog's ordinary Keel installation. Ordinary service updates
continue to use that installation.

Enable `databaseUpdater.enabled` in the Zeabur chart. Its image is built, signed and pinned
alongside Zeabur and Proxy in the release workflow, for Linux amd64 and arm64. It has its own
ServiceAccount and one Recreate Deployment. Its ClusterRole permits reading/patching CNPG
Clusters, reading nodes and pods for platform/runtime checks, and getting the namespace-local
pull Secrets explicitly referenced by a Cluster. It cannot list Secrets or mutate workloads,
pods or storage. The dashboard ServiceAccount is unchanged. Do not run multiple updater
installations against the same Clusters.

The chart publishes only the updater namespace and Deployment name in `kube-public/zeabur-info`.
Authenticated callers can get that one Deployment. `GET /api/v1/cluster/database-updater` reads
both objects as the person and summarizes native Deployment readiness; it does not use a
cluster-admin capabilities scan. Missing, unavailable and unreadable are separate states.
The UI requires ready before enabling but always permits an authorized caller to turn updates off.

The existing `autoUpdate` operation accepts a Cluster or service workload. The server fetches
the current object with the caller's credentials and patches only the four settings, clearing
legacy labels and the `matchTag` alias. Existing ownership checks, apply-time re-plan and
generation/field preconditions still apply. Status changes and unrelated annotations do not
invalidate a reviewed setting. The spec and last-applied configuration remain intact.

## Scheduled checks and writes

The controller lists Clusters periodically, including when CNPG is installed after it starts.
Each opted-in Cluster UID has its own UTC schedule. A new or changed setting waits until the
next scheduled check; restarting does not trigger an immediate refresh. Schedules are check
times, not maintenance windows: registry requests and CNPG's rollout can finish afterwards.
Missed checks while the controller is down are not replayed.

When due, the adapter creates a scoped Keel repository watcher for that one Cluster. It uses
Keel's immediate `Watch` check, without starting its shared cron scheduler. A pinned digest or
the ready database pods' runtime digests supply the baseline afresh each time. This isolates
databases sharing a tag and retries failed writes on the next check. Keel parses/authenticates
registry manifests; the adapter bounds requests and permits plain HTTP only for explicitly
configured development registries. Existing chart proxy/extra-CA settings also apply.

The candidate must keep the repository and tag, contain a valid digest, and support all
eligible Linux node platforms. The Cluster must be healthy. Before writing, the controller
re-reads UID, deletion, ownership, settings, image, catalog, credentials and placement. It
retries status conflicts with a fresh resourceVersion and patches only `spec.imageName`,
Keel's digest/update-time annotations and the change-cause annotation. Unrelated fields survive.
CloudNativePG performs the rolling restart. The controller does not verify database application
health or automatically roll back a failed rollout; failures are logged and unhealthy Clusters
are skipped. A major-version tag relies on the image publisher honoring that tag.

## Validation

`scripts/check-database-updater.sh` checks formatting, dependencies, vet, race tests and the
static build. Tests exercise the actual Keel poller, isolated schedules, conflict retries,
ownership/settings races, platform refusal and a real HTTP registry with pull-secret auth.
`scripts/check-chart.sh` checks the separate account, named readiness grant and image pin.
The full development-cluster profile builds/enables the updater from this checkout.

`web/e2e/full/databases.spec.ts` covers real Identity Developer/Viewer readiness and access,
review/apply/reload/off, status churn, last-applied retention and real S3 backups. Its future
schedule does not prove a registry-triggered rollout. Separately validate an old/new registry
tag with three real PostgreSQL instances, unchanged PVCs/data, off/future controls and a write
after switchover. Record source/image identity and stock Keel provenance for that run; earlier
patched-Keel results do not validate this implementation.
