// What a service's pods use, against what they request and are limited to: live usage from
// `GET /projects/{p}/usage` (metrics-server) and history from `.../history` (the metrics store).
// Requests and limits come from the pod template. Over its memory limit a container is
// OOM-killed; over its CPU limit it is throttled, slower but not restarted [k8s-resources]. Under
// node memory pressure, pods using more than they request are evicted first [k8s-eviction].
//
// [k8s-resources]: https://kubernetes.io/docs/concepts/configuration/manage-resources-containers/
// [k8s-eviction]: https://kubernetes.io/docs/concepts/scheduling-eviction/node-pressure-eviction/
import type { Schemas } from "@/api/client"
import type { Pod, Service } from "@/model/service"

export type ProjectUsage = Schemas["ProjectUsage"]
export type ServiceHistory = Schemas["ServiceHistory"]
export type Series = Schemas["PrometheusSeries"]
export type Restart = Schemas["PrometheusRestart"]

export const ranges = [
  "1h",
  "6h",
  "24h",
  "7d",
] as const satisfies readonly Schemas["PrometheusRange"][]
export type Range = (typeof ranges)[number]

export const splits = ["replica", "total"] as const
export type Split = (typeof splits)[number]

/** The services whose pods run on and so have usage: not CronJobs and Jobs, whose pods come and
 * go between samples. */
export const hasUsage = (service: Service) => service.kind !== "CronJob" && service.kind !== "Job"

export interface Amount {
  cpu?: number
  memory?: number
}

/** What one pod of the template requests (the containers' sum, of those that set one) and is
 * limited to (known only when every container sets one). Init containers are left out. */
export function templateAmounts(service: Service): { requests: Amount; limits: Amount } {
  const containers = service.containers.filter((c) => !c.init)
  const all = (pick: (c: (typeof containers)[number]) => number | undefined) => {
    let total = 0
    for (const c of containers) {
      const value = pick(c)
      if (value === undefined) return undefined
      total += value
    }
    return containers.length > 0 ? total : undefined
  }
  const some = (pick: (c: (typeof containers)[number]) => number | undefined) => {
    const values = containers.map(pick).filter((v): v is number => v !== undefined)
    return values.length > 0 ? values.reduce((a, b) => a + b, 0) : undefined
  }
  return {
    requests: {
      cpu: some((c) => c.resources?.requests?.cpuMillicores ?? undefined),
      memory: some((c) => c.resources?.requests?.memoryBytes ?? undefined),
    },
    limits: {
      cpu: all((c) => c.resources?.limits?.cpuMillicores ?? undefined),
      memory: all((c) => c.resources?.limits?.memoryBytes ?? undefined),
    },
  }
}

// ---------------------------------------------------------------------------------------------
// Live usage

/** A pod of the service and its sample; no sample while metrics-server has none (it measures
 * running containers only, and a new pod after two scrapes). */
export interface Measured {
  pod: Pod
  cpu?: number
  memory?: number
}

export function measured(service: Service, usage?: ProjectUsage): Measured[] {
  const byName = new Map((usage?.pods ?? []).map((p) => [p.pod, p]))
  return service.pods.map((pod) => {
    const sample = byName.get(pod.name)
    return sample ? { pod, cpu: sample.cpuMillicores, memory: sample.memoryBytes } : { pod }
  })
}

/** The sum over the pods that were measured; empty when none was. */
export function sumUsage(rows: Measured[]): Amount & { pods: number } {
  const sampled = rows.filter((r) => r.cpu !== undefined)
  if (sampled.length === 0) return { pods: 0 }
  return {
    cpu: sampled.reduce((a, r) => a + (r.cpu ?? 0), 0),
    memory: sampled.reduce((a, r) => a + (r.memory ?? 0), 0),
    pods: sampled.length,
  }
}

/** At or above this share of the memory limit a pod is close to being OOM-killed. Ours, not a
 * Kubernetes threshold. */
export const NEAR_LIMIT = 0.9

export type Note =
  | { kind: "oom" }
  | { kind: "memory-near-limit"; share: number }
  | { kind: "cpu-at-limit"; share: number }
  | { kind: "memory-over-request" }
  | { kind: "no-sample" }

/** What to say about one pod, worst first. */
export function notesFor(row: Measured, requests: Amount, limits: Amount): Note[] {
  const notes: Note[] = []
  if (row.pod.reason === "OOMKilled") notes.push({ kind: "oom" })
  if (row.memory === undefined) {
    notes.push({ kind: "no-sample" })
    return notes
  }
  if (limits.memory && row.memory / limits.memory >= NEAR_LIMIT)
    notes.push({ kind: "memory-near-limit", share: row.memory / limits.memory })
  else if (!limits.memory && requests.memory && row.memory > requests.memory)
    notes.push({ kind: "memory-over-request" })
  if (limits.cpu && row.cpu !== undefined && row.cpu / limits.cpu >= NEAR_LIMIT)
    notes.push({ kind: "cpu-at-limit", share: row.cpu / limits.cpu })
  return notes
}

// ---------------------------------------------------------------------------------------------
// History

/** The sum of every series at each time; `null` where none has a value. */
export function total(series: Series[]): Series {
  const length = series[0]?.values.length ?? 0
  const values: (number | null)[] = []
  for (let i = 0; i < length; i++) {
    let sum: number | null = null
    for (const s of series) {
      const v = s.values[i]
      if (v !== null && v !== undefined) sum = (sum ?? 0) + v
    }
    values.push(sum)
  }
  return { pod: "total", values }
}

/** How many pods had a value at the last time with any: what a per-pod request or limit is
 * multiplied by in the total. */
export function podsAtEnd(series: Series[]): number {
  const length = series[0]?.values.length ?? 0
  for (let i = length - 1; i >= 0; i--) {
    const count = series.filter((s) => s.values[i] !== null && s.values[i] !== undefined).length
    if (count > 0) return count
  }
  return 0
}

/** Whether the store holds any value for these series. */
export const hasValues = (series: Series[]) =>
  series.some((s) => s.values.some((v) => v !== null && v !== undefined))

/** The pods of a history that no longer exist (a rollout replaced them). */
export const replaced = (history: ServiceHistory, service: Service): Set<string> => {
  const now = new Set(service.pods.map((p) => p.name))
  return new Set(history.memory.map((s) => s.pod).filter((pod) => !now.has(pod)))
}

/** The colours of `series`, in order: current pods take the theme's chart colours in turn (the
 * same pod keeps its colour as long as the list does), replaced pods share the grey. */
export function colours(series: Series[], gone: Set<string>): string[] {
  let next = 0
  return series.map((s) =>
    gone.has(s.pod) ? "var(--chart-5)" : `var(--chart-${(next++ % 4) + 1})`,
  )
}

/** A service's live total and whether a measured replica runs near its memory maximum, for the
 * project's services table; nothing when no replica was measured. */
export function serviceUsage(
  service: Service,
  usage?: ProjectUsage,
): (Amount & { pods: number; nearLimit: boolean }) | undefined {
  if (!hasUsage(service) || usage?.state !== "available") return undefined
  const rows = measured(service, usage)
  const sum = sumUsage(rows)
  if (sum.pods === 0) return undefined
  const { requests, limits } = templateAmounts(service)
  const nearLimit = rows.some((row) =>
    notesFor(row, requests, limits).some((n) => n.kind === "memory-near-limit"),
  )
  return { ...sum, nearLimit }
}
