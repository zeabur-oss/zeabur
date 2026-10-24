// What each namespace's pods use across the cluster (`GET /cluster/usage`), grouped as the
// cluster page shows it: projects, other namespaces, and those Kubernetes keeps for itself.
import type { Schemas } from "@/api/client"
import { NEAR_LIMIT } from "@/model/usage"

export type ClusterUsage = Schemas["ClusterUsage"]
export type NamespaceUsage = Schemas["NamespaceUsage"]
export type Resource = "cpu" | "memory"

const field = { cpu: "cpuMillicores", memory: "memoryBytes" } as const

/** What a row leads with: measured use when metrics-server answered, else what pods reserve. */
export function lead(item: NamespaceUsage, resource: Resource): number {
  return (item.usage ?? item.requests)[field[resource]]
}

export const reserved = (item: NamespaceUsage, resource: Resource) => item.requests[field[resource]]

/** The most the namespace's pods may use together; unknown while any pod sets no limit. */
export function maximum(item: NamespaceUsage, resource: Resource): number | undefined {
  return item.pods > 0 && item.unlimited[resource] === 0 ? item.limits[field[resource]] : undefined
}

/** Measured memory at or above `NEAR_LIMIT` of a known maximum. */
export function nearMaximum(item: NamespaceUsage): boolean {
  const most = maximum(item, "memory")
  return item.usage && most !== undefined && most > 0
    ? item.usage.memoryBytes / most >= NEAR_LIMIT
    : false
}

const byLead = (resource: Resource) => (a: NamespaceUsage, b: NamespaceUsage) =>
  lead(b, resource) - lead(a, resource) || a.namespace.localeCompare(b.namespace)

export interface Groups {
  projects: NamespaceUsage[]
  others: NamespaceUsage[]
  system: NamespaceUsage[]
}

/** Each group largest first. Without the namespaces' labels, nothing counts as a project. */
export function groups(items: readonly NamespaceUsage[], resource: Resource): Groups {
  const sorted = items.toSorted(byLead(resource))
  return {
    projects: sorted.filter((i) => !i.system && i.adopted === true),
    others: sorted.filter((i) => !i.system && i.adopted !== true),
    system: sorted.filter((i) => i.system),
  }
}

export const total = (items: readonly NamespaceUsage[], resource: Resource) =>
  items.reduce((sum, i) => sum + lead(i, resource), 0)

/** The namespace that uses the most, when any uses something. */
export function largest(
  items: readonly NamespaceUsage[],
  resource: Resource,
): NamespaceUsage | undefined {
  const first = items.toSorted(byLead(resource))[0]
  return first && lead(first, resource) > 0 ? first : undefined
}

export const shownName = (item: NamespaceUsage) => item.displayName ?? item.namespace
