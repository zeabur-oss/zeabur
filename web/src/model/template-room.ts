import type { Schemas } from "@/api/client"

/** The share of the nodes' allocatable memory and CPU a template must leave free. */
const MARGIN = 0.1

export interface TemplateRoom {
  verdict: "room" | "tight"
  /** The resource the verdict is about: memory, unless CPU is the one that is short. */
  resource: "memory" | "cpu"
  /** What the template uses of that resource: bytes or millicores. */
  use: number
  /** What is free of it now, never below zero. */
  free: number
}

/**
 * Whether the cluster has room for what a template uses: its measured idle use, or what its pods
 * request; `undefined` without it or when the nodes' use cannot be read. What is in use is the larger of what the
 * pods use and what they request, since the scheduler places pods by their requests.
 */
export function templateRoom(
  idle: Pick<Schemas.IdleUse, "memoryBytes" | "cpuMillicores"> | null | undefined,
  nodes: Schemas.NodeList | undefined,
): TemplateRoom | undefined {
  if (!idle || !nodes) return undefined
  const { allocatable, usage, requests } = nodes.totals
  if (!usage && !requests) return undefined
  const used = (key: "memoryBytes" | "cpuMillicores") =>
    Math.max(usage?.[key] ?? 0, requests?.[key] ?? 0)
  const check = (key: "memoryBytes" | "cpuMillicores", use: number) => {
    const free = allocatable[key] - used(key)
    return { use, free: Math.max(0, free), fits: free - use >= allocatable[key] * MARGIN }
  }
  const memory = check("memoryBytes", idle.memoryBytes)
  const cpu = check("cpuMillicores", idle.cpuMillicores)
  if (!memory.fits)
    return { verdict: "tight", resource: "memory", use: memory.use, free: memory.free }
  if (!cpu.fits) return { verdict: "tight", resource: "cpu", use: cpu.use, free: cpu.free }
  return { verdict: "room", resource: "memory", use: memory.use, free: memory.free }
}
