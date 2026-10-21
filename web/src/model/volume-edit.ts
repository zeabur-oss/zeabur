// The Storage section's editor: a service's volumes as rows to grow, move, protect, add and
// remove, and the changes that carry them into one plan.
import type { Schemas } from "@/api/client"
import { defaultClaimName, subdomain, volumeRequest } from "@/routes/service/volume"

type Change = Schemas["Change"]

export interface VolumeRow {
  id: string
  /** The container that mounts it, when the workload has several. */
  container?: string
  mountPath: string
  size: string
  readOnly: boolean
  /** As in the cluster; absent for an added row. */
  was?: { mountPath: string; size: string; readOnly: boolean; claim: string }
  removed: boolean
  /** An added row's volume name; `null` while it follows the mount path. */
  name: string | null
  /** An added row's claim name; `null` while it follows the volume name. */
  claimName: string | null
  accessMode: Schemas["AccessMode"]
}

export type RowState = "same" | "changed" | "added" | "removed"

export type VolumeProblem =
  | "badPath"
  | "pathTaken"
  | "badSize"
  | "shrinks"
  | "badName"
  | "nameTaken"
  | "badClaimName"

export function rowState(row: VolumeRow): RowState {
  if (!row.was) return "added"
  if (row.removed) return "removed"
  return row.mountPath.trim() === row.was.mountPath &&
    row.size.trim() === row.was.size &&
    row.readOnly === row.was.readOnly
    ? "same"
    : "changed"
}

const blank = (row: VolumeRow) => !row.was && row.mountPath.trim() === "" && row.name === null

const live = (row: VolumeRow) => !row.removed && !blank(row)

export const editedCount = (rows: readonly VolumeRow[]) =>
  rows.filter((row) => !blank(row) && rowState(row) !== "same").length

const volumeName = /^[a-z0-9]([-a-z0-9]{0,61}[a-z0-9])?$/

/** The volume name an added row gets from its mount path: `/var/lib/data` is `data`. */
export function nameFromPath(mountPath: string): string {
  const last = mountPath.trim().split("/").filter(Boolean).at(-1) ?? ""
  const name = last
    .toLowerCase()
    .replace(/[^a-z0-9-]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 63)
    .replace(/-+$/, "")
  return name || "data"
}

export const nameOf = (row: VolumeRow) => row.name ?? nameFromPath(row.mountPath)

export const claimNameOf = (workload: string, row: VolumeRow) =>
  row.claimName ?? defaultClaimName(workload, nameOf(row))

const units: Record<string, number> = {
  "": 1,
  k: 1e3,
  M: 1e6,
  G: 1e9,
  T: 1e12,
  P: 1e15,
  E: 1e18,
  Ki: 2 ** 10,
  Mi: 2 ** 20,
  Gi: 2 ** 30,
  Ti: 2 ** 40,
  Pi: 2 ** 50,
  Ei: 2 ** 60,
}

/** A size in bytes, for comparing two sizes; the plan checks it again. */
export function bytesOf(size: string): number | undefined {
  const match = /^\+?(\d+(?:\.\d*)?|\.\d+)(k|Ki|M|Mi|G|Gi|T|Ti|P|Pi|E|Ei)?$/.exec(size.trim())
  if (!match) return undefined
  return Number(match[1]) * (units[match[2] ?? ""] ?? 1)
}

/** What is wrong with each row; `taken` holds the paths other mounts of the service use. */
export function problemsOf(
  rows: readonly VolumeRow[],
  taken: ReadonlySet<string> = new Set(),
): Map<string, VolumeProblem> {
  const paths = new Map<string, number>()
  const names = new Map<string, number>()
  for (const row of rows.filter(live)) {
    const path = row.mountPath.trim()
    paths.set(path, (paths.get(path) ?? 0) + 1)
    if (!row.was) names.set(nameOf(row), (names.get(nameOf(row)) ?? 0) + 1)
  }
  const problems = new Map<string, VolumeProblem>()
  for (const row of rows.filter(live)) {
    const path = row.mountPath.trim()
    const bytes = bytesOf(row.size)
    if (!path.startsWith("/")) problems.set(row.id, "badPath")
    else if ((paths.get(path) ?? 0) > 1 || taken.has(path)) problems.set(row.id, "pathTaken")
    else if (bytes === undefined || bytes <= 0) problems.set(row.id, "badSize")
    else if (row.was && bytes < (bytesOf(row.was.size) ?? 0)) problems.set(row.id, "shrinks")
    else if (!row.was && !volumeName.test(nameOf(row))) problems.set(row.id, "badName")
    else if (!row.was && (names.get(nameOf(row)) ?? 0) > 1) problems.set(row.id, "nameTaken")
    else if (!row.was && row.claimName !== null && !subdomain.test(row.claimName.trim()))
      problems.set(row.id, "badClaimName")
  }
  return problems
}

/** Removals first, so an added or moved volume may take a freed path; then mounts, sizes and
 * new volumes. */
export function changesOf(rows: readonly VolumeRow[], workload: Schemas["WorkloadRef"]): Change[] {
  const removes: Change[] = []
  const mounts: Change[] = []
  const sizes: Change[] = []
  const adds: Change[] = []
  for (const row of rows) {
    const state = rowState(row)
    const { was } = row
    if (state === "removed" && was)
      removes.push({
        op: "removeVolume",
        workload,
        container: row.container,
        mountPath: was.mountPath,
      })
    else if (state === "changed" && was) {
      const mountPath = row.mountPath.trim()
      if (mountPath !== was.mountPath || row.readOnly !== was.readOnly)
        mounts.push({
          op: "editMount",
          workload,
          container: row.container,
          mountPath: was.mountPath,
          newMountPath: mountPath === was.mountPath ? undefined : mountPath,
          readOnly: row.readOnly === was.readOnly ? undefined : row.readOnly,
        })
      const size = row.size.trim()
      if (size !== was.size) sizes.push({ op: "expandVolume", claim: was.claim, size })
    } else if (state === "added" && !blank(row))
      adds.push({
        op: "addVolume",
        workload,
        container: row.container,
        volume: volumeRequest(workload.name, {
          name: nameOf(row),
          mountPath: row.mountPath,
          size: row.size,
          claimName: row.claimName,
          accessMode: row.accessMode,
        }),
      })
  }
  return [...removes, ...mounts, ...sizes, ...adds]
}
