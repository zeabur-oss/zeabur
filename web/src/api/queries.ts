import { QueryClient, queryOptions } from "@tanstack/react-query"

import { api, ApiError, unwrap, type Schemas } from "@/api/client"

/** The longest delay `setTimeout` keeps (2^31 - 1 ms, about 24.8 days); a longer one fires at
 * once, so a server's longer ask waits this long instead. */
const MAX_TIMEOUT = 2 ** 31 - 1

/** Whether a read that failed `failures` times is tried again. A 4xx answer is the cluster's
 * decision (forbidden, not found): asking again changes nothing, except after a 429, which asks
 * to come back later. */
export function retryRead(failures: number, error: unknown): boolean {
  const refused =
    error instanceof ApiError && error.status >= 400 && error.status < 500 && error.status !== 429
  return !refused && failures < 2
}

/** How long to wait before trying a read again: all that a 429 or 503 asked for, since an earlier
 * try only spends a retry while the server still refuses; else TanStack Query's own backoff,
 * capped at 30 s (`defaultRetryDelay` in its `retryer.ts`, not exported). */
export function retryDelay(failures: number, error: unknown): number {
  if (error instanceof ApiError && error.retryAfter !== undefined)
    return Math.min(error.retryAfter * 1000, MAX_TIMEOUT)
  return Math.min(1000 * 2 ** failures, 30_000)
}

// Mutations keep the library's default of no retry: a change is never sent twice on its own.
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

/** How often a visible page asks again for what changes on its own (pods, events). */
const POLL = 5_000

/** `poll: false` for pages that read many projects at once: they read on load only. */
export interface PollOptions {
  poll?: boolean
}

export const whoamiQuery = queryOptions({
  queryKey: ["whoami"],
  queryFn: async ({ signal }) => unwrap(await api.GET("/api/v1/whoami", { signal })),
  staleTime: Infinity,
})

/** How the person is signed in to engawa: signing out, admin rights and until when. Asked again
 * while admin rights are in use, so the indicator goes when they end. */
export const sessionQuery = queryOptions({
  queryKey: ["session"],
  queryFn: async ({ signal }) => unwrap(await api.GET("/api/v1/session", { signal })),
  staleTime: 30_000,
  refetchInterval: (query) => (query.state.data?.elevation === "active" ? 15_000 : false),
})

/** Every project, with the health counts of the adopted ones. */
export const projectsQuery = queryOptions({
  queryKey: ["projects"],
  queryFn: async ({ signal }) => unwrap(await api.GET("/api/v1/projects", { signal })),
})

/** The project document: services, resources and how they relate. The service, variables and
 * objects views are cut from it. Polled while the page is visible. */
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

/** A project's Namespace: adopted or not, its details, and whether the person may change them. */
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

/** The newest events of a project (100, newest first). Polled while visible. */
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

/** ConfigMaps in full, to show where a variable's value comes from. */
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

/** How often the cluster page reads nodes and warnings again: each read lists every pod and event
 * of the cluster, so less often than a project. */
const CLUSTER_POLL = 30_000

/** Nodes with requests against allocatable, and usage when metrics-server is there. */
export const nodesQuery = queryOptions({
  queryKey: ["cluster", "nodes"],
  queryFn: async ({ signal }) => unwrap(await api.GET("/api/v1/cluster/nodes", { signal })),
  refetchInterval: CLUSTER_POLL,
  refetchIntervalInBackground: false,
})

/** The cluster's version against the support table, and newer releases from its upgrade
 * Plans. */
export const upgradesQuery = queryOptions({
  queryKey: ["cluster", "upgrades"],
  queryFn: async ({ signal }) => unwrap(await api.GET("/api/v1/cluster/upgrades", { signal })),
  staleTime: 5 * 60_000,
})

/** The last hour's warnings about the cluster itself (no namespace, a Node, or a `kube-`
 * namespace), the newest 100. */
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

/** The last hour's warnings of every namespace, the newest 500: enough to count them per project
 * on the home page. */
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

/** The cluster's Kubernetes version; readable by everyone signed in. */
export const clusterQuery = queryOptions({
  queryKey: ["cluster"],
  queryFn: async ({ signal }) => unwrap(await api.GET("/api/v1/cluster", { signal })),
  staleTime: 60_000,
})

/** What the cluster has installed; `403` for people who cannot see the whole cluster. */
export const capabilitiesQuery = queryOptions({
  queryKey: ["cluster", "capabilities"],
  queryFn: async ({ signal }) => unwrap(await api.GET("/api/v1/cluster/capabilities", { signal })),
  staleTime: 60_000,
})

/** What a project is missing, with citations and fixes. Its rules change only when objects do, so
 * it is read when the Overview opens and every minute, not polled with the document. */
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

/** What the cluster itself is missing (an IngressClass of a retired controller). */
export const clusterGapsQuery = queryOptions({
  queryKey: ["cluster", "gaps"],
  queryFn: async ({ signal }) => unwrap(await api.GET("/api/v1/cluster/gaps", { signal })),
  staleTime: 60_000,
})

/** How to install a catalogue component yourself, for a profile (the cluster's own by default). */
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

/** What to read on the Objects page: by default the kinds a project usually holds; `kinds` reads
 * only those (`<group>/<resource>`, `core` for the core group); `all` reads every kind controllers
 * do not generate. */
export interface ObjectsRead {
  kinds?: string[]
  all?: boolean
}

/** A project's kinds, each with an access hint, and the objects of the kinds read (metadata only).
 * Read while the Objects page is open and kept 45 s; never polled. Every answer sits under
 * `["project", p, "objects"]`, so one invalidation refreshes them all. */
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

/** An object's address in the API; an empty group is the core group. */
export interface ObjectAddress {
  group: string
  version: string
  resource: string
  name: string
}

/** One object as YAML, Secret values hidden. */
export const objectQuery = (project: string, address: ObjectAddress) =>
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
  })

/** A workload's releases, newest first, each with its pod template. Under the project's key, so
 * an applied plan (a rollback) refreshes them. */
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

/** The URL of a container's log as Server-Sent Events (same origin, so the cookie is sent). */
export function followUrl(project: string, pod: string, options: LogOptions): string {
  const query = new URLSearchParams({ tailLines: String(options.tailLines) })
  if (options.container) query.set("container", options.container)
  if (options.previous) query.set("previous", "true")
  if (options.timestamps) query.set("timestamps", "true")
  return `/api/v1/projects/${encodeURIComponent(project)}/pods/${encodeURIComponent(pod)}/logs/follow?${query}`
}

/** A project's backups across providers (K8up, Velero, CloudNativePG). Its own route, read only
 * where backups are shown and refreshed slowly: schedules run hours apart. */
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

/** Ready-made NetworkPolicy objects for one service (`deployment/web`); read when asked for. */
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

/** What each pod of a project uses now, from metrics-server, which measures every 15 s in its
 * stock setup: asked again as often while the page is visible. */
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

/** A service's CPU, memory and restarts over `range`, from the metrics store. Only the last hour
 * moves enough to ask again (every 30 s); longer ranges are read once. */
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

/** The metrics store history is read from, whether it answers, and who may change it. */
export const metricsStoreQuery = queryOptions({
  queryKey: ["settings", "metrics-store"],
  queryFn: async ({ signal }) =>
    unwrap(await api.GET("/api/v1/settings/metrics-store", { signal })),
})
