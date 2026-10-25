import { QueryClient, queryOptions } from "@tanstack/react-query"

import { api, ApiError, unwrap, type Schemas } from "@/api/client"

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
  queryFn: async ({ signal }) => unwrap(await api.GET("/api/v1/whoami", { signal })),
  staleTime: Infinity,
})

export const sessionQuery = queryOptions({
  queryKey: ["session"],
  queryFn: async ({ signal }) => unwrap(await api.GET("/api/v1/session", { signal })),
  staleTime: 30_000,
  refetchInterval: (query) => (query.state.data?.elevation === "active" ? 15_000 : false),
})

export const projectsQuery = queryOptions({
  queryKey: ["projects"],
  queryFn: async ({ signal }) => unwrap(await api.GET("/api/v1/projects", { signal })),
})

export const projectQuery = (project: string, { poll = true }: PollOptions = {}) =>
  queryOptions({
    queryKey: ["project", project],
    queryFn: async ({ signal }) =>
      unwrap(
        await api.GET("/api/v1/projects/{project}", { params: { path: { project } }, signal }),
      ),
    refetchInterval: poll ? POLL : false,
    refetchIntervalInBackground: false,
  })

export const namespaceQuery = (project: string) =>
  queryOptions({
    queryKey: ["project", project, "namespace"],
    queryFn: async ({ signal }) =>
      unwrap(
        await api.GET("/api/v1/projects/{project}/namespace", {
          params: { path: { project } },
          signal,
        }),
      ),
  })

export const eventsQuery = (project: string, { poll = true }: PollOptions = {}) =>
  queryOptions({
    queryKey: ["project", project, "events"],
    queryFn: async ({ signal }) =>
      unwrap(
        await api.GET("/api/v1/projects/{project}/events", {
          params: { path: { project } },
          signal,
        }),
      ),
    refetchInterval: poll ? POLL : false,
    refetchIntervalInBackground: false,
  })

export const configMapsQuery = (project: string) =>
  queryOptions({
    queryKey: ["project", project, "configmaps"],
    queryFn: async ({ signal }) =>
      unwrap(
        await api.GET("/api/v1/projects/{project}/configmaps", {
          params: { path: { project } },
          signal,
        }),
      ),
  })

export const debugQuery = (project: string, pod: string, container: string | undefined) =>
  queryOptions({
    queryKey: ["project", project, "debug", pod, container],
    queryFn: async ({ signal }) =>
      unwrap(
        await api.GET("/api/v1/projects/{project}/pods/{pod}/debug", {
          params: { path: { project, pod }, query: { container } },
          signal,
        }),
      ),
    staleTime: 0,
  })

export const toolsQuery = (project: string) =>
  queryOptions({
    queryKey: ["project", project, "tools"],
    queryFn: async ({ signal }) =>
      unwrap(
        await api.GET("/api/v1/projects/{project}/tools", {
          params: { path: { project } },
          signal,
        }),
      ),
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
      unwrap(
        await api.GET("/api/v1/projects/{project}/pods/{pod}/logs", {
          params: {
            path: { project, pod },
            query: {
              container: options.container || undefined,
              tailLines: options.tailLines,
              previous: options.previous || undefined,
              timestamps: options.timestamps || undefined,
            },
          },
          signal,
        }),
      ),
    staleTime: 0,
  })

const CLUSTER_POLL = 30_000

export const nodesQuery = queryOptions({
  queryKey: ["cluster", "nodes"],
  queryFn: async ({ signal }) => unwrap(await api.GET("/api/v1/cluster/nodes", { signal })),
  refetchInterval: CLUSTER_POLL,
  refetchIntervalInBackground: false,
  // Home and Cluster wait on these reads while they are pending; a refused read retried on every
  // mount would unmount the section that mounted it, again and again. The poll still retries.
  retryOnMount: false,
})

/** A template of Zeabur Catalog this build carries; it changes only with the build. */
export const templateQuery = (template: string) =>
  queryOptions({
    queryKey: ["cluster", "templates", template],
    queryFn: async ({ signal }) =>
      unwrap(
        await api.GET("/api/v1/cluster/templates/{template}", {
          params: { path: { template } },
          signal,
        }),
      ),
    staleTime: Infinity,
  })

export const clusterHealthQuery = queryOptions({
  queryKey: ["cluster", "health"],
  queryFn: async ({ signal }) => unwrap(await api.GET("/api/v1/cluster/health", { signal })),
  refetchInterval: CLUSTER_POLL,
  refetchIntervalInBackground: false,
  retryOnMount: false,
})

export const upgradesQuery = queryOptions({
  queryKey: ["cluster", "upgrades"],
  queryFn: async ({ signal }) => unwrap(await api.GET("/api/v1/cluster/upgrades", { signal })),
  staleTime: 5 * 60_000,
  retryOnMount: false,
})

export const clusterWarningsQuery = queryOptions({
  queryKey: ["cluster", "warnings"],
  queryFn: async ({ signal }) =>
    unwrap(
      await api.GET("/api/v1/cluster/events", {
        params: { query: { type: "Warning", sinceSeconds: 3600, scope: "cluster" } },
        signal,
      }),
    ),
  refetchInterval: CLUSTER_POLL,
  refetchIntervalInBackground: false,
})

export const recentWarningsQuery = queryOptions({
  queryKey: ["cluster", "warnings", "counted"],
  queryFn: async ({ signal }) =>
    unwrap(
      await api.GET("/api/v1/cluster/events", {
        params: { query: { type: "Warning", sinceSeconds: 3600, limit: 500 } },
        signal,
      }),
    ),
  refetchInterval: CLUSTER_POLL,
  refetchIntervalInBackground: false,
})

export const clusterQuery = queryOptions({
  queryKey: ["cluster"],
  queryFn: async ({ signal }) => unwrap(await api.GET("/api/v1/cluster", { signal })),
  staleTime: 60_000,
  // The banner waits on this read; when the cluster is away it polls every 15 s anyway.
  retry: 1,
  retryOnMount: false,
})

export const capabilitiesQuery = queryOptions({
  queryKey: ["cluster", "capabilities"],
  queryFn: async ({ signal }) => unwrap(await api.GET("/api/v1/cluster/capabilities", { signal })),
  staleTime: 60_000,
  retryOnMount: false,
})

export const configReloadQuery = queryOptions({
  queryKey: ["cluster", "config-reload"],
  queryFn: async ({ signal }) => unwrap(await api.GET("/api/v1/cluster/config-reload", { signal })),
  staleTime: 60_000,
  retryOnMount: false,
})

export const projectGapsQuery = (project: string) =>
  queryOptions({
    queryKey: ["project", project, "gaps"],
    queryFn: async ({ signal }) =>
      unwrap(
        await api.GET("/api/v1/projects/{project}/gaps", {
          params: { path: { project } },
          signal,
        }),
      ),
    staleTime: 60_000,
  })

export const clusterGapsQuery = queryOptions({
  queryKey: ["cluster", "gaps"],
  queryFn: async ({ signal }) => unwrap(await api.GET("/api/v1/cluster/gaps", { signal })),
  staleTime: 60_000,
})

export const installQuery = (capability: string, component: string, profile?: string) =>
  queryOptions({
    queryKey: ["cluster", "install", capability, component, profile ?? "default"],
    queryFn: async ({ signal }) =>
      unwrap(
        await api.GET("/api/v1/cluster/components/{capability}/{component}/install", {
          params: { path: { capability, component }, query: profile ? { profile } : {} },
          signal,
        }),
      ),
    staleTime: Infinity,
  })

export const joinQuery = queryOptions({
  queryKey: ["cluster", "join"],
  queryFn: async ({ signal }) => unwrap(await api.GET("/api/v1/cluster/join", { signal })),
  staleTime: 30_000,
  retryOnMount: false,
})

export const nodePoliciesQuery = queryOptions({
  queryKey: ["cluster", "node-policies"],
  queryFn: async ({ signal }) => unwrap(await api.GET("/api/v1/cluster/node-policies", { signal })),
  refetchInterval: CLUSTER_POLL,
  refetchIntervalInBackground: false,
  retryOnMount: false,
})

/** How the baseline network policies change to let every node in, and the token; writes nothing. */
export async function planNodePolicies(signal?: AbortSignal) {
  return unwrap(await api.POST("/api/v1/cluster/node-policies/plan", { signal }))
}

export async function applyNodePolicies(token: string) {
  return unwrap(await api.POST("/api/v1/cluster/node-policies/apply", { body: { token } }))
}

export const clusterBackupsQuery = queryOptions({
  queryKey: ["cluster", "backups"],
  queryFn: async ({ signal }) => unwrap(await api.GET("/api/v1/cluster/backups", { signal })),
  refetchInterval: CLUSTER_POLL,
  refetchIntervalInBackground: false,
  retryOnMount: false,
})

/** What a change to Velero's location or schedule writes, and the token; writes nothing. */
export async function planClusterBackups(
  request: Schemas["BackupsPlanRequest"],
  signal?: AbortSignal,
) {
  return unwrap(await api.POST("/api/v1/cluster/backups/plan", { body: request, signal }))
}

export async function applyClusterBackups(token: string) {
  return unwrap(await api.POST("/api/v1/cluster/backups/apply", { body: { token } }))
}

export const clusterIssuersQuery = queryOptions({
  queryKey: ["cluster", "issuers"],
  queryFn: async ({ signal }) => unwrap(await api.GET("/api/v1/cluster/issuers", { signal })),
  retryOnMount: false,
})

/** What creating or changing a DNS-01 issuer writes, and the token; writes nothing. */
export async function planClusterIssuer(request: Schemas["IssuerRequest"], signal?: AbortSignal) {
  return unwrap(await api.POST("/api/v1/cluster/issuers/plan", { body: request, signal }))
}

export async function applyClusterIssuer(token: string) {
  return unwrap(await api.POST("/api/v1/cluster/issuers/apply", { body: { token } }))
}

/** The dates of the certificate in a project's `kubernetes.io/tls` Secret; never its key. */
export const certificateDatesQuery = (project: string, secret: string) =>
  queryOptions({
    queryKey: ["project", project, "certificate", secret],
    queryFn: async ({ signal }) =>
      unwrap(
        await api.GET("/api/v1/projects/{project}/certificates/{secret}", {
          params: { path: { project, secret } },
          signal,
        }),
      ),
    retry: false,
  })

/** Where alerts go: a project's channels, or the cluster's when `project` is absent. */
export const notificationsQuery = (project?: string) =>
  queryOptions({
    queryKey: project ? ["project", project, "notifications"] : ["cluster", "notifications"],
    queryFn: async ({ signal }) =>
      unwrap(
        project
          ? await api.GET("/api/v1/projects/{project}/notifications", {
              params: { path: { project } },
              signal,
            })
          : await api.GET("/api/v1/cluster/notifications", { signal }),
      ),
    refetchInterval: CLUSTER_POLL,
    refetchIntervalInBackground: false,
    retryOnMount: false,
  })

/** What adding, changing, testing or removing a channel writes, and the token; writes nothing. */
export async function planNotifications(
  project: string | undefined,
  request: Schemas["NotificationsPlanRequest"],
  signal?: AbortSignal,
) {
  return unwrap(
    project
      ? await api.POST("/api/v1/projects/{project}/notifications/plan", {
          params: { path: { project } },
          body: request,
          signal,
        })
      : await api.POST("/api/v1/cluster/notifications/plan", { body: request, signal }),
  )
}

export async function applyNotifications(project: string | undefined, token: string) {
  return unwrap(
    project
      ? await api.POST("/api/v1/projects/{project}/notifications/apply", {
          params: { path: { project } },
          body: { token },
        })
      : await api.POST("/api/v1/cluster/notifications/apply", { body: { token } }),
  )
}

/** What stopping scheduling, resuming it or maintenance does to a node's pods; writes nothing. */
export async function planNodeMaintenance(
  node: string,
  action: Schemas["NodeAction"],
  signal?: AbortSignal,
) {
  return unwrap(
    await api.POST("/api/v1/cluster/nodes/{node}/maintenance/plan", {
      params: { path: { node } },
      body: { action },
      signal,
    }),
  )
}

export async function applyNodeMaintenance(node: string, body: Schemas["MaintenanceApply"]) {
  return unwrap(
    await api.POST("/api/v1/cluster/nodes/{node}/maintenance/apply", {
      params: { path: { node } },
      body,
    }),
  )
}

/** One more eviction pass of a maintenance plan. */
export async function evictNodePods(node: string, body: Schemas["MaintenanceApply"]) {
  return unwrap(
    await api.POST("/api/v1/cluster/nodes/{node}/maintenance/evict", {
      params: { path: { node } },
      body,
    }),
  )
}

/** `kinds` are `<group>/<resource>` (`core` for the core group); `all` reads every kind controllers
 * do not generate. */
export interface ObjectsRead {
  kinds?: string[]
  all?: boolean
}

export const objectsQuery = (project: string, read: ObjectsRead = {}) =>
  queryOptions({
    queryKey: ["project", project, "objects", read.all ? "*" : (read.kinds ?? []).join(",")],
    queryFn: async ({ signal }) =>
      unwrap(
        await api.GET("/api/v1/projects/{project}/objects", {
          params: {
            path: { project },
            query: {
              kinds: read.kinds?.length ? read.kinds.join(",") : undefined,
              all: read.all ? true : undefined,
            },
          },
          signal,
        }),
      ),
    staleTime: 45_000,
  })

/** The kinds the person may create in the project, and the cluster-scoped ones Create refuses. */
export const creatableQuery = (project: string) =>
  queryOptions({
    queryKey: ["project", project, "creatable"],
    queryFn: async ({ signal }) =>
      unwrap(
        await api.GET("/api/v1/projects/{project}/creatable", {
          params: { path: { project } },
          signal,
        }),
      ),
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
      unwrap(
        await api.GET("/api/v1/projects/{project}/objects/{group}/{version}/{resource}/{name}", {
          params: {
            path: {
              project,
              group: address.group || "core",
              version: address.version,
              resource: address.resource,
              name: address.name,
            },
          },
          signal,
        }),
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
      unwrap(
        await api.GET("/api/v1/projects/{project}/events", {
          params: { path: { project }, query: { regarding: `${kind}/${name}` } },
          signal,
        }),
      ),
    refetchInterval: POLL,
    refetchIntervalInBackground: false,
  })

/** Under the project's key, so an applied plan (a rollback) refreshes them. */
export const releasesQuery = (project: string, kind: string, name: string) =>
  queryOptions({
    queryKey: ["project", project, "releases", kind, name],
    queryFn: async ({ signal }) =>
      unwrap(
        await api.GET("/api/v1/projects/{project}/services/{kind}/{name}/releases", {
          params: { path: { project, kind: kind.toLowerCase(), name } },
          signal,
        }),
      ),
  })

const servicePath = (project: string, kind: string, name: string) => ({
  path: { project, kind: kind.toLowerCase(), name },
})

/** Each domain and private address of a service, followed hop by hop. */
export const traceQuery = (project: string, kind: string, name: string) =>
  queryOptions({
    queryKey: ["project", project, "trace", kind, name],
    queryFn: async ({ signal }) =>
      unwrap(
        await api.GET("/api/v1/projects/{project}/services/{kind}/{name}/trace", {
          params: servicePath(project, kind, name),
          signal,
        }),
      ),
    refetchInterval: 15_000,
    refetchIntervalInBackground: false,
  })

/** How far the current rollout has come; polled while it moves. */
export const rolloutQuery = (project: string, kind: string, name: string) =>
  queryOptions({
    queryKey: ["project", project, "rollout", kind, name],
    queryFn: async ({ signal }) =>
      unwrap(
        await api.GET("/api/v1/projects/{project}/services/{kind}/{name}/rollout", {
          params: servicePath(project, kind, name),
          signal,
        }),
      ),
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
    queryFn: async ({ signal }) =>
      unwrap(
        await api.GET("/api/v1/projects/{project}/services/{kind}/{name}/runs", {
          params: servicePath(project, "CronJob", name),
          signal,
        }),
      ),
    refetchInterval: (query) =>
      query.state.data?.runs.some((r) => r.status === "running") ? 3_000 : POLL,
    refetchIntervalInBackground: false,
  })

export const serviceTimelineQuery = (project: string, kind: string, name: string) =>
  queryOptions({
    queryKey: ["project", project, "timeline", kind, name],
    queryFn: async ({ signal }) =>
      unwrap(
        await api.GET("/api/v1/projects/{project}/services/{kind}/{name}/timeline", {
          params: servicePath(project, kind, name),
          signal,
        }),
      ),
    refetchInterval: 15_000,
    refetchIntervalInBackground: false,
  })

export const projectTimelineQuery = (project: string) =>
  queryOptions({
    queryKey: ["project", project, "timeline"],
    queryFn: async ({ signal }) =>
      unwrap(
        await api.GET("/api/v1/projects/{project}/timeline", {
          params: { path: { project } },
          signal,
        }),
      ),
    refetchInterval: 15_000,
    refetchIntervalInBackground: false,
  })

/** A service's problems, trace, events and previous logs as one text, read when it is copied. */
export const evidenceQuery = (project: string, kind: string, name: string) =>
  queryOptions({
    queryKey: ["project", project, "evidence", kind, name],
    queryFn: async ({ signal }) =>
      unwrap(
        await api.GET("/api/v1/projects/{project}/services/{kind}/{name}/evidence", {
          params: servicePath(project, kind, name),
          signal,
        }),
      ),
    staleTime: 0,
  })

export const projectIncidentsQuery = (project: string) =>
  queryOptions({
    queryKey: ["project", project, "incidents"],
    queryFn: async ({ signal }) =>
      unwrap(
        await api.GET("/api/v1/projects/{project}/incidents", {
          params: { path: { project } },
          signal,
        }),
      ),
    refetchInterval: POLL,
    refetchIntervalInBackground: false,
  })

export const clusterIncidentsQuery = queryOptions({
  queryKey: ["cluster", "incidents"],
  queryFn: async ({ signal }) => unwrap(await api.GET("/api/v1/cluster/incidents", { signal })),
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
    queryFn: async ({ signal }) =>
      unwrap(
        await api.GET("/api/v1/projects/{project}/builds", {
          params: { path: { project } },
          signal,
        }),
      ),
    refetchInterval: 30_000,
    refetchIntervalInBackground: false,
  })

export const backupsQuery = (project: string) =>
  queryOptions({
    queryKey: ["project", project, "backups"],
    queryFn: async ({ signal }) =>
      unwrap(
        await api.GET("/api/v1/projects/{project}/backups", {
          params: { path: { project } },
          signal,
        }),
      ),
    refetchInterval: 60_000,
    refetchIntervalInBackground: false,
  })

export const projectAlertsQuery = (project: string) =>
  queryOptions({
    queryKey: ["project", project, "alerts"],
    queryFn: async ({ signal }) =>
      unwrap(
        await api.GET("/api/v1/projects/{project}/alerts", {
          params: { path: { project } },
          signal,
        }),
      ),
    refetchInterval: 30_000,
    refetchIntervalInBackground: false,
  })

export const clusterAlertsQuery = queryOptions({
  queryKey: ["cluster", "alerts"],
  queryFn: async ({ signal }) => unwrap(await api.GET("/api/v1/cluster/alerts", { signal })),
  refetchInterval: CLUSTER_POLL,
  refetchIntervalInBackground: false,
})

export const databasesQuery = (project: string) =>
  queryOptions({
    queryKey: ["project", project, "databases"],
    queryFn: async ({ signal }) =>
      unwrap(
        await api.GET("/api/v1/projects/{project}/databases", {
          params: { path: { project } },
          signal,
        }),
      ),
    refetchInterval: 15_000,
    refetchIntervalInBackground: false,
  })

export const networkPresetsQuery = (project: string, service: string) =>
  queryOptions({
    queryKey: ["project", project, "network-presets", service],
    queryFn: async ({ signal }) =>
      unwrap(
        await api.GET("/api/v1/projects/{project}/network-policy/presets", {
          params: { path: { project }, query: { service } },
          signal,
        }),
      ),
    staleTime: 60_000,
  })

export const usageQuery = (project: string) =>
  queryOptions({
    queryKey: ["project", project, "usage"],
    queryFn: async ({ signal }) =>
      unwrap(
        await api.GET("/api/v1/projects/{project}/usage", {
          params: { path: { project } },
          signal,
        }),
      ),
    refetchInterval: 15_000,
    refetchIntervalInBackground: false,
  })

/** The StorageClasses a new volume can use, and what each claim of the project is on. */
export const storageQuery = (project: string) =>
  queryOptions({
    queryKey: ["project", project, "storage"],
    queryFn: async ({ signal }) =>
      unwrap(
        await api.GET("/api/v1/projects/{project}/storage", {
          params: { path: { project } },
          signal,
        }),
      ),
    staleTime: 30_000,
  })

/** How full the project's volumes are; each read asks the kubelets of the nodes they are on. */
export const volumesQuery = (project: string) =>
  queryOptions({
    queryKey: ["project", project, "volumes"],
    queryFn: async ({ signal }) =>
      unwrap(
        await api.GET("/api/v1/projects/{project}/volumes", {
          params: { path: { project } },
          signal,
        }),
      ),
    refetchInterval: 60_000,
    refetchIntervalInBackground: false,
  })

export const historyQuery = (
  project: string,
  kind: string,
  name: string,
  range: Schemas["PrometheusRange"],
) =>
  queryOptions({
    queryKey: ["project", project, "history", kind, name, range],
    queryFn: async ({ signal }) =>
      unwrap(
        await api.GET("/api/v1/projects/{project}/services/{kind}/{name}/history", {
          params: { path: { project, kind: kind.toLowerCase(), name }, query: { range } },
          signal,
        }),
      ),
    staleTime: range === "1h" ? 10_000 : 5 * 60_000,
    refetchInterval: range === "1h" ? 30_000 : false,
    refetchIntervalInBackground: false,
  })

export const logHistoryQuery = (
  project: string,
  kind: string,
  name: string,
  range: Schemas["PrometheusRange"],
  container: string | undefined,
) =>
  queryOptions({
    queryKey: ["project", project, "log-history", kind, name, range, container],
    queryFn: async ({ signal }) =>
      unwrap(
        await api.GET("/api/v1/projects/{project}/services/{kind}/{name}/logs/history", {
          params: {
            path: { project, kind: kind.toLowerCase(), name },
            query: { range, container },
          },
          signal,
        }),
      ),
    staleTime: 10_000,
  })

export const clusterNameQuery = queryOptions({
  queryKey: ["settings", "cluster-name"],
  queryFn: async ({ signal }) => unwrap(await api.GET("/api/v1/settings/cluster-name", { signal })),
})

export const domainsQuery = queryOptions({
  queryKey: ["settings", "domains"],
  queryFn: async ({ signal }) => unwrap(await api.GET("/api/v1/settings/domains", { signal })),
})

export const metricsStoreQuery = queryOptions({
  queryKey: ["settings", "metrics-store"],
  queryFn: async ({ signal }) =>
    unwrap(await api.GET("/api/v1/settings/metrics-store", { signal })),
})

/** What an image declares (its ports, its user), read from its registry; `unreadable` says why
 * not. A `POST`, since it may carry registry credentials. */
export const imageInspectionQuery = (project: string, body: Schemas["InspectRequest"]) =>
  queryOptions({
    queryKey: ["project", project, "image", body],
    queryFn: async ({ signal }) =>
      unwrap(
        await api.POST("/api/v1/projects/{project}/images/inspect", {
          params: { path: { project } },
          body,
          signal,
        }),
      ),
    staleTime: 5 * 60_000,
    retry: false,
  })

export const objectStorageQuery = (project: string) =>
  queryOptions({
    queryKey: ["project", project, "object-storage"],
    queryFn: async ({ signal }) =>
      unwrap(
        await api.GET("/api/v1/projects/{project}/object-storage", {
          params: { path: { project } },
          signal,
        }),
      ),
    refetchInterval: 15_000,
    refetchIntervalInBackground: false,
  })
