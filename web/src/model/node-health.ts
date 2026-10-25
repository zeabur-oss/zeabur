// What is wrong with a node, in the order it hurts: down, a pressure the kubelet acts on, memory or
// disk nearly full, then what only limits new pods (cordoned, reservations). Fill is measured use
// against allocatable when metrics exist; reservations (requests) only say whether new pods fit.
import type { Schemas } from "@/api/client"
import { healthRank, type Health } from "@/model/service"
import { severity } from "@/model/severity"

type NodeView = Schemas["NodeView"]
type DiskUsage = Schemas["DiskUsage"]

/** Share of allocatable reserved above which new pods may not fit. */
export const TIGHT = 0.9
/** Memory in use: the kernel kills processes as it runs out. */
export const MEMORY = { warn: 0.85, fail: 0.95 }
/** The kubelet starts evicting pods below 10% free (`nodefs.available<10%`). */
export const DISK = { warn: 0.85, fail: 0.9 }
/** A volume: the app's writes fail when it is full. */
export const VOLUME = { warn: 0.85, fail: 0.95 }

export const share = (part: number | undefined, whole: number) =>
  whole > 0 && part !== undefined ? part / whole : undefined

export function fillHealth(
  ratio: number | undefined,
  { warn, fail }: { warn: number; fail: number },
): Health {
  if (ratio === undefined) return "ok"
  return ratio >= fail ? "fail" : ratio >= warn ? "warn" : "ok"
}

export const diskShare = (disk: DiskUsage | null | undefined) =>
  disk ? share(disk.capacityBytes - disk.availableBytes, disk.capacityBytes) : undefined

export type IssueKind =
  | "notReady"
  | "memoryPressure"
  | "diskPressure"
  | "pidPressure"
  | "networkUnavailable"
  | "memoryFull"
  | "diskFull"
  | "cordoned"
  | "draining"
  | "maintenance"
  | "reservedMemory"
  | "reservedCpu"

export interface NodeIssue {
  kind: IssueKind
  node: string
  health: Health
  /** The share it is about: memory or disk in use, or reserved. */
  share?: number
}

const pressures: Record<string, { kind: IssueKind; health: Health }> = {
  MemoryPressure: { kind: "memoryPressure", health: "fail" },
  DiskPressure: { kind: "diskPressure", health: "fail" },
  PIDPressure: { kind: "pidPressure", health: "warn" },
  NetworkUnavailable: { kind: "networkUnavailable", health: "fail" },
}

const order: IssueKind[] = [
  "notReady",
  "memoryPressure",
  "diskPressure",
  "networkUnavailable",
  "memoryFull",
  "diskFull",
  "pidPressure",
  "cordoned",
  "draining",
  "maintenance",
  "reservedMemory",
  "reservedCpu",
]

/** Takes, or is about to take, services down; the rest only limit what new pods can do. */
export const isProblem = (issue: NodeIssue) =>
  !(
    ["cordoned", "draining", "maintenance", "reservedMemory", "reservedCpu"] as IssueKind[]
  ).includes(issue.kind)

/** A cordoned node, by what is still on it: replicas leaving, none left to move (maintenance), or
 * replicas that stay until someone moves them. Unknown when its pods cannot be read. */
function cordonedKind(node: NodeView): IssueKind {
  if ((node.leavingPods ?? 0) > 0) return "draining"
  if (node.movablePods === 0) return "maintenance"
  return "cordoned"
}

const cordonedHealth: Partial<Record<IssueKind, Health>> = {
  draining: "progress",
  maintenance: "idle",
}

const byIssue = (a: NodeIssue, b: NodeIssue) =>
  healthRank[a.health] - healthRank[b.health] ||
  order.indexOf(a.kind) - order.indexOf(b.kind) ||
  (b.share ?? 0) - (a.share ?? 0)

export function memoryUsed(node: NodeView): number | undefined {
  return share(node.usage?.memoryBytes, node.allocatable.memoryBytes)
}

/** Every issue of a node, worst first. A node that is down has only that one. */
export function nodeIssues(node: NodeView): NodeIssue[] {
  const name = node.name
  if (node.ready !== "True")
    return [{ kind: "notReady", node: name, health: severity.nodeNotReady }]
  const issues: NodeIssue[] = []
  for (const condition of node.problems) {
    const pressure = pressures[condition.type]
    if (pressure) issues.push({ ...pressure, node: name })
  }
  const has = (kind: IssueKind) => issues.some((i) => i.kind === kind)
  const memory = memoryUsed(node)
  const memoryHealth = fillHealth(memory, MEMORY)
  if (memoryHealth !== "ok" && !has("memoryPressure"))
    issues.push({ kind: "memoryFull", node: name, health: memoryHealth, share: memory })
  const disk = diskShare(node.disk)
  const diskHealth = fillHealth(disk, DISK)
  if (diskHealth !== "ok" && !has("diskPressure"))
    issues.push({ kind: "diskFull", node: name, health: diskHealth, share: disk })
  if (node.cordoned) {
    const kind = cordonedKind(node)
    issues.push({ kind, node: name, health: cordonedHealth[kind] ?? "warn" })
  }
  if (node.requests) {
    const memoryReserved = share(node.requests.memoryBytes, node.allocatable.memoryBytes)
    if (memoryReserved !== undefined && memoryReserved >= TIGHT)
      issues.push({
        kind: "reservedMemory",
        node: name,
        health: severity.overReservation,
        share: memoryReserved,
      })
    const cpuReserved = share(node.requests.cpuMillicores, node.allocatable.cpuMillicores)
    if (cpuReserved !== undefined && cpuReserved >= TIGHT)
      issues.push({
        kind: "reservedCpu",
        node: name,
        health: severity.overReservation,
        share: cpuReserved,
      })
  }
  return issues.toSorted(byIssue)
}

export interface IssueGroup {
  kind: IssueKind
  health: Health
  /** Worst first. */
  issues: NodeIssue[]
}

/** One group per kind of issue, so 12 nodes over-reserved on memory read as one line. Worst
 * first, then the larger group. */
export function groupIssues(issues: readonly NodeIssue[]): IssueGroup[] {
  const groups = new Map<IssueKind, NodeIssue[]>()
  for (const issue of issues) groups.set(issue.kind, [...(groups.get(issue.kind) ?? []), issue])
  return [...groups.entries()]
    .map(([kind, list]) => {
      const sorted = list.toSorted(byIssue)
      return { kind, health: sorted[0]?.health ?? "ok", issues: sorted }
    })
    .toSorted(
      (a, b) =>
        healthRank[a.health] - healthRank[b.health] ||
        order.indexOf(a.kind) - order.indexOf(b.kind) ||
        b.issues.length - a.issues.length,
    )
}

/** How bad a node is, for sorting worst first: its worst issue, then how full it is. */
function nodeSeverity(node: NodeView): [number, number, number] {
  const worst = nodeIssues(node)[0]
  const fullest = Math.max(memoryUsed(node) ?? 0, diskShare(node.disk) ?? 0)
  return [
    worst ? healthRank[worst.health] : healthRank.ok,
    worst ? order.indexOf(worst.kind) : order.length,
    -fullest,
  ]
}

export function byNodeSeverity(a: NodeView, b: NodeView): number {
  const [x, y] = [nodeSeverity(a), nodeSeverity(b)]
  return x[0] - y[0] || x[1] - y[1] || x[2] - y[2] || a.name.localeCompare(b.name)
}

/** The node the totals would hide: the one with the worst problem, when there is more than one
 * node. */
export function worstNode(
  nodes: readonly NodeView[],
): { node: NodeView; issue: NodeIssue } | undefined {
  if (nodes.length < 2) return undefined
  const node = nodes.toSorted(byNodeSeverity)[0]
  const issue = node ? nodeIssues(node)[0] : undefined
  return node && issue && isProblem(issue) ? { node, issue } : undefined
}
