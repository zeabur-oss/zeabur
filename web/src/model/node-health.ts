// How full a node or a volume is. The thresholds and the judgement come from the server
// (`GET /cluster/nodes`: `thresholds`, each node's `issues`).
import type { Schemas } from "@/api/client"
import type { Health } from "@/model/service"

type NodeView = Schemas.NodeView
type DiskUsage = Schemas.DiskUsage

export const share = (part: number | undefined, whole: number) =>
  whole > 0 && part !== undefined ? part / whole : undefined

export function fillHealth(ratio: number | undefined, { warn, fail }: Schemas.Threshold): Health {
  if (ratio === undefined) return "ok"
  return ratio >= fail ? "fail" : ratio >= warn ? "warn" : "ok"
}

export const diskShare = (disk: DiskUsage | null | undefined) =>
  disk ? share(disk.capacityBytes - disk.availableBytes, disk.capacityBytes) : undefined

export function memoryUsed(node: NodeView): number | undefined {
  return share(node.usage?.memoryBytes, node.allocatable.memoryBytes)
}
