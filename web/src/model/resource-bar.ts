// Where a container's request, limit and usage sit between a small floor and the largest node,
// on a logarithmic scale: 50m and 4 cores both stay readable on one bar.
import type { Schemas } from "@/api/client"
import { parseAmount, type Dimension } from "@/lib/amount"
import type { Service } from "@/model/service"

/** The left end of the scale: 10m of CPU, 16 MiB of memory. */
const floors: Record<Dimension, number> = { cpu: 0.01, bytes: 16 * 2 ** 20 }

export interface BarShares {
  /** Where the request is, or 0 without one. */
  from: number
  /** Where the limit is, or 1 without one. */
  to: number
  /** Where the usage is; absent without a sample. */
  mark?: number
}

/** `request` and `limit` as typed; `usage` and `largest` in cores or bytes. `undefined` when
 * there is no node to scale to. */
export function barShares(
  dimension: Dimension,
  request: string,
  limit: string,
  usage: number | undefined,
  largest: number | undefined,
): BarShares | undefined {
  const floor = floors[dimension]
  if (largest === undefined || largest <= floor) return undefined
  const at = (value: number) =>
    Math.min(Math.max(Math.log(value / floor) / Math.log(largest / floor), 0), 1)
  const position = (text: string) => {
    const value = parseAmount(text, dimension)
    return value === undefined || value <= 0 ? undefined : at(value)
  }
  const from = position(request) ?? 0
  const to = Math.max(position(limit) ?? 1, from)
  return { from, to, mark: usage === undefined ? undefined : at(Math.max(usage, floor)) }
}

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
