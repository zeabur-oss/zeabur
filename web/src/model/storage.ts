// Where a volume's data is: the StorageClass it is on, whether that class lets it grow, and the
// node it is kept on.
import type { Schemas } from "@/api/client"

export type StorageClass = Schemas["StorageClassView"]
export type ProjectStorage = Schemas["ProjectStorage"]
type ClaimStorage = Schemas["ClaimStorage"]

/** The class a new volume gets when none is named. */
const defaultClass = (storage: ProjectStorage | undefined): StorageClass | undefined =>
  storage?.classes.find((c) => c.default)

/** The class `name` names, or the default for none; `undefined` when it cannot be read. */
export function classNamed(
  storage: ProjectStorage | undefined,
  name: string | null | undefined,
): StorageClass | undefined {
  if (!name) return defaultClass(storage)
  return storage?.classes.find((c) => c.name === name)
}

export const claimStorage = (
  storage: ProjectStorage | undefined,
  claim: string,
): ClaimStorage | undefined => storage?.claims.find((c) => c.claim === claim)

/** Whether a volume on `name` can grow; `undefined` when its class cannot be read, and the
 * cluster decides. */
export const canGrow = (
  storage: ProjectStorage | undefined,
  name: string | null | undefined,
): boolean | undefined => classNamed(storage, name)?.allowExpansion

export type Placement =
  /** Kept on this node: its replicas run only there. */
  | { kind: "node"; node: string }
  /** Kept on some nodes, which the volume does not name by hostname. */
  | { kind: "pinned" }
  /** Made on the node of the first replica that uses it, then kept there. */
  | { kind: "firstUse" }
  /** Follows its replicas from node to node. */
  | { kind: "moves" }

/** Where a claim's data is: from its PersistentVolume when it could be read, else from its
 * class, with `podNode` (a node a replica that mounts it runs on) for a local volume. */
export function placementOf(
  claim: ClaimStorage | undefined,
  storageClass: StorageClass | undefined,
  podNode?: string,
): Placement | undefined {
  if (claim?.pinned === true)
    return claim.node ? { kind: "node", node: claim.node } : { kind: "pinned" }
  if (claim?.pinned === false) return { kind: "moves" }
  if (storageClass?.kind === "replicated") return { kind: "moves" }
  if (storageClass?.kind !== "local") return undefined
  if (claim?.volume || podNode)
    return podNode ? { kind: "node", node: podNode } : { kind: "pinned" }
  return { kind: "firstUse" }
}

/** Classes as a picker offers them: the default first, then by name. */
export const offeredClasses = (storage: ProjectStorage | undefined): StorageClass[] =>
  (storage?.classes ?? []).toSorted(
    (a, b) => Number(b.default) - Number(a.default) || a.name.localeCompare(b.name),
  )

/** The class to send for a choice: none for the default, so the cluster picks it. */
export const chosenClass = (
  storage: ProjectStorage | undefined,
  choice: string | undefined,
): string | undefined => (choice && choice !== defaultClass(storage)?.name ? choice : undefined)
