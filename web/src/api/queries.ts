import { QueryClient, queryOptions } from "@tanstack/react-query"

import { ApiError, type Schemas } from "@/api/client"
import * as zeabur from "@/api/zeabur"

/** `setTimeout` fires at once for a delay above 2^31 - 1 ms. */
const MAX_TIMEOUT = 2 ** 31 - 1

export function retryRead(failures: number, error: unknown): boolean {
  const refused =
    error instanceof ApiError && error.status >= 400 && error.status < 500 && error.status !== 429
  return !refused && failures < 2
}

/** Else TanStack Query's default backoff (`defaultRetryDelay`, not exported). */
export function retryDelay(failures: number, error: unknown): number {
  if (error instanceof ApiError && error.retryAfter !== undefined)
    return Math.min(error.retryAfter * 1000, MAX_TIMEOUT)
  return Math.min(1000 * 2 ** failures, 30_000)
}

// Mutations are never retried: a change is never sent twice on its own.
export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: retryRead,
      retryDelay,
      staleTime: 10_000,
      refetchOnWindowFocus: false,
    },
  },
})

const POLL = 5_000

export interface PollOptions {
  poll?: boolean
}

export const whoamiQuery = queryOptions({
  queryKey: ["whoami"],
  queryFn: async ({ signal }) => await zeabur.getWhoAmI({ signal }),
  staleTime: Infinity,
})

export const sessionQuery = queryOptions({
  queryKey: ["session"],
  queryFn: async ({ signal }) => await zeabur.getSession({ signal }),
  staleTime: 30_000,
})

export const projectsQuery = queryOptions({
  queryKey: ["projects"],
  queryFn: async ({ signal }) => await zeabur.listProjects({ signal }),
})

export const projectQuery = (project: string, { poll = true }: PollOptions = {}) =>
  queryOptions({
    queryKey: ["project", project],
    queryFn: async ({ signal }) => await zeabur.getProject(project, { signal }),
    refetchInterval: poll ? POLL : false,
    refetchIntervalInBackground: false,
  })

export const namespaceQuery = (project: string) =>
  queryOptions({
    queryKey: ["project", project, "namespace"],
    queryFn: async ({ signal }) => await zeabur.getProjectNamespace(project, { signal }),
  })

export const eventsQuery = (project: string, { poll = true }: PollOptions = {}) =>
  queryOptions({
    queryKey: ["project", project, "events"],
    queryFn: async ({ signal }) => await zeabur.listProjectEvents(project, undefined, { signal }),
    refetchInterval: poll ? POLL : false,
    refetchIntervalInBackground: false,
  })

export const configMapsQuery = (project: string) =>
  queryOptions({
    queryKey: ["project", project, "configmaps"],
    queryFn: async ({ signal }) => await zeabur.listConfigMaps(project, { signal }),
  })

export const volumeRestoresQuery = (project: string, claim: string) =>
  queryOptions({
    queryKey: ["project", project, "volume-restores", claim],
    queryFn: async ({ signal }) => await zeabur.getVolumeRestores(project, claim, { signal }),
    refetchInterval: POLL,
    refetchIntervalInBackground: false,
  })

export const debugQuery = (project: string, pod: string, container: string | undefined) =>
  queryOptions({
    queryKey: ["project", project, "debug", pod, container],
    queryFn: async ({ signal }) =>
      await zeabur.getDebugContainer(project, pod, { container }, { signal }),
    staleTime: 0,
  })

export const toolsQuery = (project: string) =>
  queryOptions({
    queryKey: ["project", project, "tools"],
    queryFn: async ({ signal }) => await zeabur.listToolsPods(project, { signal }),
  })

export interface LogOptions {
  container?: string
  tailLines: number
  previous: boolean
  timestamps: boolean
}

export const logsQuery = (project: string, pod: string, options: LogOptions) =>
  queryOptions({
    queryKey: ["project", project, "logs", pod, options],
    queryFn: async ({ signal }) =>
      await zeabur.getPodLogs(
        project,
        pod,
        {
          container: options.container || undefined,
          tailLines: options.tailLines,
          previous: options.previous || undefined,
          timestamps: options.timestamps || undefined,
        },
        { signal },
      ),
    staleTime: 0,
  })

const CLUSTER_POLL = 30_000

export const nodesQuery = queryOptions({
  queryKey: ["cluster", "nodes"],
  queryFn: async ({ signal }) => await zeabur.listNodes({ signal }),
  refetchInterval: CLUSTER_POLL,
  refetchIntervalInBackground: false,
  // Home and Cluster wait on these reads while they are pending; a refused read retried on every
  // mount would unmount the section that mounted it, again and again. The poll still retries.
  retryOnMount: false,
})

/** The templates of Zeabur Catalog this build carries, with whether the cluster serves the
 * components each one needs. */
export const templatesQuery = queryOptions({
  queryKey: ["cluster", "templates"],
  queryFn: async ({ signal }) => await zeabur.listTemplates({ signal }),
})

/** A template of Zeabur Catalog this build carries. Its idle use changes only with the build. */
export const templateQuery = (template: string) =>
  queryOptions({
    queryKey: ["cluster", "templates", template],
    queryFn: async ({ signal }) => await zeabur.getTemplate(template, { signal }),
    staleTime: Infinity,
  })

export const clusterUsageQuery = queryOptions({
  queryKey: ["cluster", "usage"],
  queryFn: async ({ signal }) => await zeabur.getClusterUsage({ signal }),
  refetchInterval: CLUSTER_POLL,
  refetchIntervalInBackground: false,
  retryOnMount: false,
})

export const clusterHealthQuery = queryOptions({
  queryKey: ["cluster", "health"],
  queryFn: async ({ signal }) => await zeabur.getClusterHealth({ signal }),
  refetchInterval: CLUSTER_POLL,
  refetchIntervalInBackground: false,
  retryOnMount: false,
})

export const upgradesQuery = queryOptions({
  queryKey: ["cluster", "upgrades"],
  queryFn: async ({ signal }) => await zeabur.getUpgrades({ signal }),
  staleTime: 5 * 60_000,
  retryOnMount: false,
})

export const clusterWarningsQuery = queryOptions({
  queryKey: ["cluster", "warnings"],
  queryFn: async ({ signal }) =>
    await zeabur.listClusterEvents(
      { type: "Warning", sinceSeconds: 3600, scope: "cluster" },
      { signal },
    ),
  refetchInterval: CLUSTER_POLL,
  refetchIntervalInBackground: false,
})

export const recentWarningsQuery = queryOptions({
  queryKey: ["cluster", "warnings", "counted"],
  queryFn: async ({ signal }) =>
    await zeabur.listClusterEvents({ type: "Warning", sinceSeconds: 3600, limit: 500 }, { signal }),
  refetchInterval: CLUSTER_POLL,
  refetchIntervalInBackground: false,
})

/** How often an open page reads the running version again, so an upgrade or a changed
 * `updateCheck` reaches it without a reload. */
export const ZEABUR_EVERY = 60 * 60_000

export const zeaburQuery = queryOptions({
  queryKey: ["cluster", "zeabur"],
  queryFn: async ({ signal }) => await zeabur.getZeabur({ signal }),
  staleTime: ZEABUR_EVERY,
  refetchInterval: ZEABUR_EVERY,
  retryOnMount: false,
})

export const clusterQuery = queryOptions({
  queryKey: ["cluster"],
  queryFn: async ({ signal }) => await zeabur.getCluster({ signal }),
  staleTime: 60_000,
  // The banner waits on this read; when the cluster is away it polls every 15 s anyway.
  retry: 1,
  retryOnMount: false,
})

export const capabilitiesQuery = queryOptions({
  queryKey: ["cluster", "capabilities"],
  queryFn: async ({ signal }) => await zeabur.listCapabilities({ signal }),
  staleTime: 60_000,
  retryOnMount: false,
})

export const configReloadQuery = queryOptions({
  queryKey: ["cluster", "config-reload"],
  queryFn: async ({ signal }) => await zeabur.getConfigReload({ signal }),
  staleTime: 60_000,
  retryOnMount: false,
})

export const projectGapsQuery = (project: string) =>
  queryOptions({
    queryKey: ["project", project, "gaps"],
    queryFn: async ({ signal }) => await zeabur.listProjectGaps(project, { signal }),
    staleTime: 60_000,
  })

export const clusterGapsQuery = queryOptions({
  queryKey: ["cluster", "gaps"],
  queryFn: async ({ signal }) => await zeabur.listClusterGaps({ signal }),
  staleTime: 60_000,
})

export const clusterBackupsQuery = queryOptions({
  queryKey: ["cluster", "backups"],
  queryFn: async ({ signal }) => await zeabur.getClusterBackups({ signal }),
  refetchInterval: CLUSTER_POLL,
  refetchIntervalInBackground: false,
  retryOnMount: false,
})

/** What a change to Velero's location or schedule writes, and the token; writes nothing. */
export async function planClusterBackups(
  request: Schemas.BackupsPlanRequest,
  signal?: AbortSignal,
) {
  return await zeabur.planClusterBackups(request, { signal })
}

/** The projects a backup holds that no longer exist; reading them downloads the backup's lists. */
export const deletedBackupProjectsQuery = (backup: string) =>
  queryOptions({
    queryKey: ["cluster", "backups", backup, "deleted-projects"],
    queryFn: async ({ signal }) => await zeabur.listDeletedBackupProjects(backup, { signal }),
    retry: false,
    staleTime: 60_000,
  })

export async function applyClusterBackups(token: string) {
  return await zeabur.applyClusterBackups({ token })
}

export const clusterIssuersQuery = queryOptions({
  queryKey: ["cluster", "issuers"],
  queryFn: async ({ signal }) => await zeabur.getClusterIssuers({ signal }),
  retryOnMount: false,
})

/** The Gateways a project's new HTTPRoute can attach to; `readable` is false when the person may
 * not list the cluster's Gateways. */
export const projectGatewaysQuery = (project: string) =>
  queryOptions({
    queryKey: ["project", project, "gateways"],
    queryFn: async ({ signal }) => await zeabur.listProjectGateways(project, { signal }),
    retry: false,
  })

/** The dates of the certificate in a project's `kubernetes.io/tls` Secret; never its key. */
export const certificateDatesQuery = (project: string, secret: string) =>
  queryOptions({
    queryKey: ["project", project, "certificate", secret],
    queryFn: async ({ signal }) => await zeabur.getCertificateDates(project, secret, { signal }),
    retry: false,
  })

/** Where alerts go: a project's channels, or the cluster's when `project` is absent. */
export const notificationsQuery = (project?: string) =>
  queryOptions({
    queryKey: project ? ["project", project, "notifications"] : ["cluster", "notifications"],
    queryFn: async ({ signal }) =>
      project
        ? await zeabur.getProjectNotifications(project, { signal })
        : await zeabur.getClusterNotifications({ signal }),
    refetchInterval: CLUSTER_POLL,
    refetchIntervalInBackground: false,
    retryOnMount: false,
  })

/** What adding, changing, testing or removing a channel writes, and the token; writes nothing. */
export async function planNotifications(
  project: string | undefined,
  request: Schemas.NotificationsPlanRequest,
  signal?: AbortSignal,
) {
  return project
    ? await zeabur.planProjectNotifications(project, request, { signal })
    : await zeabur.planClusterNotifications(request, { signal })
}

export async function applyNotifications(project: string | undefined, token: string) {
  return project
    ? await zeabur.applyProjectNotifications(project, { token })
    : await zeabur.applyClusterNotifications({ token })
}

/** What stopping or resuming scheduling does to a node; writes nothing. */
export async function planNodeMaintenance(
  node: string,
  action: Schemas.NodeAction,
  signal?: AbortSignal,
) {
  return await zeabur.planNodeMaintenance(node, { action }, { signal })
}

export async function applyNodeMaintenance(node: string, body: Schemas.MaintenanceApply) {
  return await zeabur.applyNodeMaintenance(node, body)
}

/** A Secret's values, read as the person; never cached. */
export async function revealSecret(project: string, name: string): Promise<Record<string, string>> {
  const answer = await zeabur.revealSecret(project, name, { cache: "no-store" })
  return Object.fromEntries(Object.entries(answer.values).map(([key, value]) => [key, value.value]))
}

/** `kinds` are `<group>/<resource>` (`core` for the core group); `all` reads every kind controllers
 * do not generate. */
export interface ObjectsRead {
  kinds?: string[]
  all?: boolean
  /** Only objects with this label, `<key>=<value>`. */
  label?: string
}

export const objectsQuery = (project: string, read: ObjectsRead = {}) =>
  queryOptions({
    queryKey: [
      "project",
      project,
      "objects",
      read.all ? "*" : (read.kinds ?? []).join(","),
      ...(read.label ? [read.label] : []),
    ],
    queryFn: async ({ signal }) =>
      await zeabur.listObjects(
        project,
        {
          kinds: read.kinds?.length ? read.kinds.join(",") : undefined,
          all: read.all ? true : undefined,
          label: read.label,
        },
        { signal },
      ),
    staleTime: 45_000,
  })

/** The kinds the person may create in the project, and the cluster-scoped ones Create refuses. */
export const creatableQuery = (project: string) =>
  queryOptions({
    queryKey: ["project", project, "creatable"],
    queryFn: async ({ signal }) => await zeabur.listCreatable(project, { signal }),
    staleTime: 5 * 60_000,
  })

/** An empty group is the core group. */
export interface ObjectAddress {
  group: string
  version: string
  resource: string
  name: string
}

export const objectQuery = (
  project: string,
  address: ObjectAddress,
  { poll = false }: PollOptions = {},
) =>
  queryOptions({
    queryKey: ["project", project, "object", address],
    queryFn: async ({ signal }) =>
      await zeabur.getObject(
        project,
        address.group || "core",
        address.version,
        address.resource,
        address.name,
        { signal },
      ),
    staleTime: 0,
    refetchInterval: poll ? POLL : false,
    refetchIntervalInBackground: false,
  })

/** `kind` is the object's kind (`Deployment`), as events name what they are about. */
export const objectEventsQuery = (project: string, kind: string, name: string) =>
  queryOptions({
    queryKey: ["project", project, "events", kind, name],
    queryFn: async ({ signal }) =>
      await zeabur.listProjectEvents(project, { regarding: `${kind}/${name}` }, { signal }),
    refetchInterval: POLL,
    refetchIntervalInBackground: false,
  })

/** Under the project's key, so an applied plan (a rollback) refreshes them. */
export const releasesQuery = (project: string, kind: string, name: string) =>
  queryOptions({
    queryKey: ["project", project, "releases", kind, name],
    queryFn: async ({ signal }) =>
      await zeabur.listServiceReleases(project, kind.toLowerCase(), name, { signal }),
  })

/** Each domain and private address of a service, followed hop by hop. */
export const traceQuery = (project: string, kind: string, name: string) =>
  queryOptions({
    queryKey: ["project", project, "trace", kind, name],
    queryFn: async ({ signal }) =>
      await zeabur.traceService(project, kind.toLowerCase(), name, { signal }),
    refetchInterval: 15_000,
    refetchIntervalInBackground: false,
  })

/** How far the current rollout has come; polled while it moves. */
export const rolloutQuery = (project: string, kind: string, name: string) =>
  queryOptions({
    queryKey: ["project", project, "rollout", kind, name],
    queryFn: async ({ signal }) =>
      await zeabur.getServiceRollout(project, kind.toLowerCase(), name, { signal }),
    refetchInterval: (query) =>
      query.state.data?.status === "progressing" || query.state.data?.status === "waitsForDeletion"
        ? 2_000
        : POLL,
    refetchIntervalInBackground: false,
  })

/** A CronJob's runs, newest first; polled faster while one runs. */
export const taskRunsQuery = (project: string, name: string) =>
  queryOptions({
    queryKey: ["project", project, "runs", name],
    queryFn: async ({ signal }) => await zeabur.listTaskRuns(project, "cronjob", name, { signal }),
    refetchInterval: (query) =>
      query.state.data?.runs.some((r) => r.status === "running") ? 3_000 : POLL,
    refetchIntervalInBackground: false,
  })

export const serviceTimelineQuery = (project: string, kind: string, name: string) =>
  queryOptions({
    queryKey: ["project", project, "timeline", kind, name],
    queryFn: async ({ signal }) =>
      await zeabur.getServiceTimeline(project, kind.toLowerCase(), name, { signal }),
    refetchInterval: 15_000,
    refetchIntervalInBackground: false,
  })

export const projectTimelineQuery = (project: string) =>
  queryOptions({
    queryKey: ["project", project, "timeline"],
    queryFn: async ({ signal }) => await zeabur.getProjectTimeline(project, { signal }),
    refetchInterval: 15_000,
    refetchIntervalInBackground: false,
  })

/** A service's problems, trace, events and previous logs as one text, read when it is copied. */
export const evidenceQuery = (project: string, kind: string, name: string) =>
  queryOptions({
    queryKey: ["project", project, "evidence", kind, name],
    queryFn: async ({ signal }) =>
      await zeabur.getServiceEvidence(project, kind.toLowerCase(), name, { signal }),
    staleTime: 0,
  })

export const projectIncidentsQuery = (project: string) =>
  queryOptions({
    queryKey: ["project", project, "incidents"],
    queryFn: async ({ signal }) => await zeabur.listProjectIncidents(project, { signal }),
    refetchInterval: POLL,
    refetchIntervalInBackground: false,
  })

export const clusterIncidentsQuery = queryOptions({
  queryKey: ["cluster", "incidents"],
  queryFn: async ({ signal }) => await zeabur.listClusterIncidents({ signal }),
  refetchInterval: CLUSTER_POLL,
  refetchIntervalInBackground: false,
})

/** Same origin, so EventSource sends the cookie. */
export function followUrl(project: string, pod: string, options: LogOptions): string {
  const query = new URLSearchParams({ tailLines: String(options.tailLines) })
  if (options.container) query.set("container", options.container)
  if (options.previous) query.set("previous", "true")
  if (options.timestamps) query.set("timestamps", "true")
  return `/api/v1/projects/${encodeURIComponent(project)}/pods/${encodeURIComponent(pod)}/logs/follow?${query}`
}

export const buildsQuery = (project: string) =>
  queryOptions({
    queryKey: ["project", project, "builds"],
    queryFn: async ({ signal }) => await zeabur.getProjectBuilds(project, { signal }),
    refetchInterval: 30_000,
    refetchIntervalInBackground: false,
  })

export const backupsQuery = (project: string) =>
  queryOptions({
    queryKey: ["project", project, "backups"],
    queryFn: async ({ signal }) => await zeabur.getProjectBackups(project, { signal }),
    refetchInterval: 60_000,
    refetchIntervalInBackground: false,
  })

export const projectAlertsQuery = (project: string) =>
  queryOptions({
    queryKey: ["project", project, "alerts"],
    queryFn: async ({ signal }) => await zeabur.getProjectAlerts(project, { signal }),
    refetchInterval: 30_000,
    refetchIntervalInBackground: false,
  })

export const clusterAlertsQuery = queryOptions({
  queryKey: ["cluster", "alerts"],
  queryFn: async ({ signal }) => await zeabur.getClusterAlerts({ signal }),
  refetchInterval: CLUSTER_POLL,
  refetchIntervalInBackground: false,
})

export const databasesQuery = (project: string) =>
  queryOptions({
    queryKey: ["project", project, "databases"],
    queryFn: async ({ signal }) => await zeabur.getProjectDatabases(project, { signal }),
    refetchInterval: 15_000,
    refetchIntervalInBackground: false,
  })

export const networkPresetsQuery = (project: string, service: string) =>
  queryOptions({
    queryKey: ["project", project, "network-presets", service],
    queryFn: async ({ signal }) =>
      await zeabur.listNetworkPolicyPresets(project, { service }, { signal }),
    staleTime: 60_000,
  })

export const usageQuery = (project: string) =>
  queryOptions({
    queryKey: ["project", project, "usage"],
    queryFn: async ({ signal }) => await zeabur.getProjectUsage(project, { signal }),
    refetchInterval: 15_000,
    refetchIntervalInBackground: false,
  })

/** The StorageClasses a new volume can use, and what each claim of the project is on. */
export const storageQuery = (project: string) =>
  queryOptions({
    queryKey: ["project", project, "storage"],
    queryFn: async ({ signal }) => await zeabur.getProjectStorage(project, { signal }),
    staleTime: 30_000,
  })

/** How full the project's volumes are; each read asks the kubelets of the nodes they are on. */
export const volumesQuery = (project: string) =>
  queryOptions({
    queryKey: ["project", project, "volumes"],
    queryFn: async ({ signal }) => await zeabur.getVolumeUsage(project, { signal }),
    refetchInterval: 60_000,
    refetchIntervalInBackground: false,
  })

export const historyQuery = (
  project: string,
  kind: string,
  name: string,
  range: Schemas.PrometheusRange,
) =>
  queryOptions({
    queryKey: ["project", project, "history", kind, name, range],
    queryFn: async ({ signal }) =>
      await zeabur.getServiceHistory(project, kind.toLowerCase(), name, { range }, { signal }),
    staleTime: range === "1h" ? 10_000 : 5 * 60_000,
    refetchInterval: range === "1h" ? 30_000 : false,
    refetchIntervalInBackground: false,
  })

export const logHistoryQuery = (
  project: string,
  kind: string,
  name: string,
  range: Schemas.PrometheusRange,
  container: string | undefined,
) =>
  queryOptions({
    queryKey: ["project", project, "log-history", kind, name, range, container],
    queryFn: async ({ signal }) =>
      await zeabur.getServiceLogHistory(
        project,
        kind.toLowerCase(),
        name,
        { range, container },
        { signal },
      ),
    staleTime: 10_000,
  })

export const clusterNameQuery = queryOptions({
  queryKey: ["settings", "cluster-name"],
  queryFn: async ({ signal }) => await zeabur.getClusterName({ signal }),
})

export const domainsQuery = queryOptions({
  queryKey: ["settings", "domains"],
  queryFn: async ({ signal }) => await zeabur.getDomains({ signal }),
})

export const signInSettingQuery = queryOptions({
  queryKey: ["settings", "sign-in"],
  queryFn: async ({ signal }) => await zeabur.getSignInSetting({ signal }),
})

export const metricsStoreQuery = queryOptions({
  queryKey: ["settings", "metrics-store"],
  queryFn: async ({ signal }) => await zeabur.getMetricsStore({ signal }),
})

/** What an image declares (its ports, its user), read from its registry; `unreadable` says why
 * not. A `POST`, since it may carry registry credentials. */
export const imageInspectionQuery = (project: string, body: Schemas.InspectRequest) =>
  queryOptions({
    queryKey: ["project", project, "image", body],
    queryFn: async ({ signal }) => await zeabur.inspectImage(project, body, { signal }),
    staleTime: 5 * 60_000,
    retry: false,
  })

export const objectStorageQuery = (project: string) =>
  queryOptions({
    queryKey: ["project", project, "object-storage"],
    queryFn: async ({ signal }) => await zeabur.getProjectObjectStorage(project, { signal }),
    refetchInterval: 15_000,
    refetchIntervalInBackground: false,
  })
