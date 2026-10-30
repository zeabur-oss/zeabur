// What deleting a project takes and what holds a project being deleted, in the terms the plan
// preview and the project page show them.
import type { Schemas } from "@/api/client"

type Run = Schemas.BackupRun
type Held = Schemas.HeldObject

/** A backup this recent does not call for another before the project goes. */
const RECENT_MS = 24 * 60 * 60 * 1000

/** What happens to a volume's data: `Retain` keeps the PersistentVolume, `Delete` deletes it. */
export type DataFate = "deleted" | "kept" | "unknown"

export function dataFate(volume: Schemas.VolumeDeletion): DataFate {
  switch (volume.reclaimPolicy) {
    case "Delete":
      return "deleted"
    case "Retain":
      return "kept"
    default:
      return "unknown"
  }
}

/** Whether `run` is a backup that succeeded within the last day. */
export function recentBackup(run: Run | null | undefined, now = Date.now()): boolean {
  if (!run || run.outcome !== "succeeded" || !run.finished) return false
  const finished = new Date(run.finished).getTime()
  return !Number.isNaN(finished) && now - finished < RECENT_MS
}

/** The deletion warning of a plan, when it deletes the project. */
export function projectDeletionOf(
  warnings: Schemas.PlanWarning[],
): Extract<Schemas.PlanWarning, { code: "deletesProject" }> | undefined {
  for (const warning of warnings) {
    if (warning.code === "deletesProject") return warning
  }
  return undefined
}

/** The kubectl commands that show why `namespace` is still there: the namespace's conditions,
 * then each held object with its finalizers. */
export function deletingCommands(namespace: string, held: Held[]): string {
  const objects = held.map((h) => {
    const resource = h.group ? `${h.resource}.${h.group}` : h.resource
    return `kubectl get ${resource} ${h.name} -n ${namespace} -o yaml`
  })
  return [`kubectl get namespace ${namespace} -o yaml`, ...objects].join("\n") + "\n"
}
