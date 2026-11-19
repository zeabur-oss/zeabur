// What undoes an applied plan. Undo is offered only where the inverse is a plain change of its
// own: scaling back to the number of pods there were, and deleting an object just created, at
// the resourceVersion its creation returned (so an object someone changed since is kept). It is
// planned and applied again like any change
// (https://www.nngroup.com/articles/user-control-and-freedom/).
import type { Schemas } from "@/api/client"

type Change = Schemas.Change
type WorkloadRef = Schemas.WorkloadRef
type Applied = Schemas.Applied

/** A workload's pods as the project document last reported them. */
export interface Pods {
  desired: number
  ready: number
}

/** Looks up a workload's pods; `undefined` when the page does not know them. */
export type PodsOf = (workload: WorkloadRef) => Pods | undefined

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value)
}

/** The changes that undo `changes`, or `undefined` when they cannot be taken back as a change of
 * their own: a plan of scale changes, to the pod counts known when it was planned, or of
 * creations, by deleting what `applied` says was created at the version it was created at. */
export function undoOf(
  changes: readonly Change[],
  podsOf: PodsOf,
  applied: readonly Applied[] = [],
): Change[] | undefined {
  if (changes.length === 0) return undefined
  // Objects written as YAML: every object the apply wrote, the last created deleted first.
  if (changes.every((c) => c.op === "create" && c.yaml)) {
    const deletes = applied.map((a) =>
      deletionOf({ ...a.target, resourceVersion: a.resourceVersion }),
    )
    return deletes.length > 0 && deletes.every((d): d is Change => d !== undefined)
      ? deletes.toReversed()
      : undefined
  }
  const undo: Change[] = []
  for (const change of changes) {
    const one = undoOne(change, podsOf, applied)
    if (!one) return undefined
    undo.push(one)
  }
  return undo
}

/** The change that undoes one creation or scale. */
function undoOne(change: Change, podsOf: PodsOf, applied: readonly Applied[]): Change | undefined {
  if (change.op === "create") {
    const object: Record<string, unknown> = isRecord(change.object) ? change.object : {}
    const name = isRecord(object.metadata) ? object.metadata.name : undefined
    const created = applied.find((a) => a.target.kind === object.kind && a.target.name === name)
    return created
      ? deletionOf({ ...created.target, resourceVersion: created.resourceVersion })
      : undefined
  }
  if (change.op !== "scale") return undefined
  const before = podsOf(change.workload)?.desired
  if (before === undefined || before === change.replicas) return undefined
  return { op: "scale", workload: change.workload, replicas: before }
}

/** The deletion of an object just created, at the version it was created at; `undefined` when
 * that version is not known. */
export function deletionOf(created: {
  apiVersion: string
  plural: string
  name: string
  resourceVersion?: string | null
}): Change | undefined {
  if (!created.resourceVersion) return undefined
  const [group = "", version = created.apiVersion] = created.apiVersion.includes("/")
    ? created.apiVersion.split("/")
    : ["", created.apiVersion]
  return {
    op: "delete",
    resource: { group, version, resource: created.plural },
    name: created.name,
    resourceVersion: created.resourceVersion,
  }
}
