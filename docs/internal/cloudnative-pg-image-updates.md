# CloudNativePG image updates

The database form reuses the service form's policy and schedule controls and the same four Keel annotations. CloudNativePG uses only same-tag refreshes: its PostgreSQL images already have major-version rolling tags, and selecting another major or image flavor is a database migration. Digest pins remain until Keel resolves a replacement on the saved schedule. ClusterImageCatalog refresh would need another reconciler and does not reuse Keel's per-resource schedule.

The existing reviewed object edit writes the annotations with the caller's credentials. It retains the read resourceVersion, so a stale full-object form is refused before planning instead of reverting an intervening image or storage change. The plan's existing ownership checks and apply preconditions still apply. No new endpoint, plan variant, controller or Zeabur service-account grant is needed.

Keel needs a CloudNativePG Cluster adapter: discovery and its existing informer/cache, a synthetic PostgreSQL container for the existing registry poller, and a writer that patches only imageName and Keel's update annotations. Poll events are scoped to the Cluster UID and saved schedule; a Cluster is not updated by another resource's poll or by an unscoped webhook. Discovery happens at startup, so restart Keel after installing CloudNativePG. Catalog-managed images and images without an explicit PostgreSQL major tag are not tracked.

The writer keeps the repository and tag, requires a valid digest, and rechecks UID, deletion, ownership labels, owner references, update settings, imageName and imageCatalogRef after each conflict. An unrelated spec change survives the image patch. Existing poll jobs and registry platform checks are reused; a failed write remains eligible for retry on the next scheduled check. CloudNativePG handles the rollout; Keel's health and rollback limitations still apply.

## Dependency order

The catalog currently pins Keel 0.22.4 / chart v1.2.3. That release does not watch Clusters. The companion Keel change must be released and validated first. Then update its catalog entry with the released chart and image digest and get/list/watch/patch permissions for postgresql.cnpg.io Clusters. Move Zeabur's catalog pin and regenerate its corpus in the separate catalog-pin contribution required by CONTRIBUTING.md. Do not invent a release version or digest, or deploy a custom dashboard image to work around this dependency.

Request: [issue 1004, CloudNativePG follow-up](https://github.com/zeabur-oss/zeabur/issues/1004#issuecomment-7216236644). The previous service implementation is [PR 1019](https://github.com/zeabur-oss/zeabur/pull/1019).
