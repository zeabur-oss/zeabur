// When a container's usage says its CPU or memory amounts need a look.
import type { Schemas } from "@/api/client"
import { parseAmount, type Dimension } from "@/lib/amount"
import type { Service } from "@/model/service"

/** The share of the memory limit from which a container counts as near it. */
const nearShare = 0.9

/** Each container's usage in its busiest pod, by container name: cores and bytes. */
export function busiest(
  service: Service,
  usage: Schemas.ProjectUsage | undefined,
): Map<string, { cpu: number; memory: number }> {
  const pods = new Set(service.pods.map((p) => p.name))
  const most = new Map<string, { cpu: number; memory: number }>()
  for (const pod of usage?.pods ?? []) {
    if (!pods.has(pod.pod)) continue
    for (const c of pod.containers) {
      const before = most.get(c.name)
      most.set(c.name, {
        cpu: Math.max(before?.cpu ?? 0, c.cpuMillicores / 1000),
        memory: Math.max(before?.memory ?? 0, c.memoryBytes),
      })
    }
  }
  return most
}

export interface UsageWarnings {
  /** Memory only: the usage is close to the limit, where the container is restarted. */
  nearLimit?: { limit: number }
  /** The usage is above what the container reserves. */
  aboveRequest?: { request: number }
}

/** What to tell about `usage` (cores or bytes) against the amounts as typed; nothing without a
 * usage or a readable amount. */
export function usageWarnings(
  dimension: Dimension,
  request: string,
  limit: string,
  usage: number | undefined,
): UsageWarnings {
  if (usage === undefined) return {}
  const reserved = parseAmount(request, dimension)
  const capped = parseAmount(limit, dimension)
  const warnings: UsageWarnings = {}
  if (dimension === "bytes" && capped !== undefined && capped > 0 && usage >= capped * nearShare)
    warnings.nearLimit = { limit: capped }
  if (reserved !== undefined && reserved > 0 && usage > reserved)
    warnings.aboveRequest = { request: reserved }
  return warnings
}

/** A container's four amounts as typed in the form. */
export interface TypedAmounts {
  cpuRequest: string
  memoryRequest: string
  cpuLimit: string
  memoryLimit: string
}

/** What the busiest pod's usage says about the CPU or the memory amounts as typed; `undefined`
 * when there is nothing to say. */
export function rowWarnings(
  dimension: Dimension,
  amounts: TypedAmounts | undefined,
  sample: { cpu: number; memory: number } | undefined,
): (UsageWarnings & { used: number }) | undefined {
  if (!amounts || !sample) return undefined
  const cpu = dimension === "cpu"
  const used = cpu ? sample.cpu : sample.memory
  const found = usageWarnings(
    dimension,
    cpu ? amounts.cpuRequest : amounts.memoryRequest,
    cpu ? amounts.cpuLimit : amounts.memoryLimit,
    used,
  )
  return found.nearLimit || found.aboveRequest ? { ...found, used } : undefined
}
