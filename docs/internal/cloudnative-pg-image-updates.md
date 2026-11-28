# CloudNativePG image updates

The database API returns `imageUpdates` when Zeabur can manage a Cluster's settings. The
projection, planner and updater share the CloudNativePG rules in `zeabur-model`. Helm, Flux and
Argo CD keep control of the objects and fields they manage. The existing `autoUpdate` plan edits
the scheduling annotations with the caller's credentials. An existing setting can be turned off
even when the image is no longer eligible or the updater is unavailable.

The Helm chart starts `zeabur database-updater` with a dedicated ServiceAccount when
`databaseUpdater.enabled` is set. The dashboard reads the named Deployment as the caller to
show its readiness. The updater can list and patch Clusters, list Nodes and Pods and get the
namespace-local pull Secrets named by a Cluster. Ordinary services continue to use Keel.

The updater records the next UTC check for each opted-in Cluster UID in memory. A new or
changed schedule and a restart wait for the next check. At that time it reads the existing
tag through Zeabur's OCI registry client. It checks the published digest against the image
pin or running PostgreSQL Pods and verifies support for the eligible Linux node architectures.
A changed tag always retains its repository, PostgreSQL major and image flavor as named.

Before patching, the updater reads the Cluster again and checks its identity, ownership,
settings, placement and health. A resource-version precondition prevents a concurrent change
from being overwritten. It patches only `spec.imageName` and the digest, update-time and
change-cause annotations. CloudNativePG performs the rollout. Failures are logged and the
next scheduled check uses the persisted state. There is no automatic rollback.
