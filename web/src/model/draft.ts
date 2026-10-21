// Edits gathered on a service's Networking, Variables and Settings tabs before one plan: which
// part each belongs to, and how a later edit of the same thing replaces an earlier one.
import type { Schemas } from "@/api/client"
import type { DraftPart } from "@/routes/search"

type Change = Schemas["Change"]

export function partOf(change: Change): DraftPart | undefined {
  switch (change.op) {
    case "env":
    case "data":
      return "variables"
    case "addDomain":
    case "renameDomain":
    case "removeDomain":
      return "networking"
    case "addVolume":
    case "expandVolume":
    case "editMount":
    case "removeVolume":
      return "storage"
    case "scale":
    case "resources":
      return "compute"
    case "probe":
      return "health-checks"
    default:
      return undefined
  }
}

/** Whether every change of an ask waits in the draft rather than being planned at once. */
export const draftable = (changes: readonly Change[]) =>
  changes.length > 0 && changes.every((c) => partOf(c) !== undefined)

const workloadKey = (w: Schemas["WorkloadRef"]) => `${w.kind}/${w.name}`

/** Two changes with one key edit the same thing: the later one stands. */
function keyOf(change: Change): string | undefined {
  switch (change.op) {
    case "env":
      return `env:${workloadKey(change.workload)}:${change.container ?? ""}`
    case "data":
      return `data:${change.kind}/${change.name}`
    case "scale":
      return `scale:${workloadKey(change.workload)}`
    case "resources":
      return `resources:${workloadKey(change.workload)}:${change.container ?? ""}`
    case "probe":
      return `probe:${workloadKey(change.workload)}:${change.container ?? ""}:${change.probe}`
    case "addDomain":
    case "removeDomain":
      return `domain:${change.hostname}`
    case "renameDomain":
      return `domain:${change.from}`
    case "expandVolume":
      return `expand:${change.claim}`
    case "editMount":
    case "removeVolume":
      return `mount:${workloadKey(change.workload)}:${change.mountPath}`
    default:
      return undefined
  }
}

function combine(before: Change, after: Change): Change {
  if (before.op === "env" && after.op === "env") {
    const names = new Set(after.changes.map((e) => e.name))
    return {
      ...after,
      changes: [...before.changes.filter((e) => !names.has(e.name)), ...after.changes],
    }
  }
  if (before.op === "data" && after.op === "data") {
    const keys = new Set(after.changes.map((e) => e.key))
    return {
      ...after,
      changes: [...before.changes.filter((e) => !keys.has(e.key)), ...after.changes],
    }
  }
  return after
}

export function mergeChanges(staged: readonly Change[], added: readonly Change[]): Change[] {
  const result = [...staged]
  for (const change of added) {
    const key = keyOf(change)
    const at = key === undefined ? -1 : result.findIndex((c) => keyOf(c) === key)
    if (at === -1) result.push(change)
    else result[at] = combine(result[at] as Change, change)
  }
  return result
}

/** Edits, as a person counts them: each variable or key is one. */
export const editCount = (changes: readonly Change[]) =>
  changes.reduce((n, c) => n + (c.op === "env" || c.op === "data" ? c.changes.length : 1), 0)

/** The staged changes less those `dropped` stands for. */
export function dropChanges(staged: readonly Change[], dropped: readonly Change[]): Change[] {
  const keys = new Set(dropped.map(keyOf).filter((k) => k !== undefined))
  return staged.filter((c) => {
    const key = keyOf(c)
    return key === undefined ? !dropped.includes(c) : !keys.has(key)
  })
}

export type DomainEdit =
  | Extract<Change, { op: "addDomain" }>
  | Extract<Change, { op: "renameDomain" }>
  | Extract<Change, { op: "removeDomain" }>

/** The staged domain edits of a workload: domains to add, and edits of its routes' hosts by
 * `route/host`. */
export function stagedDomains(
  changes: readonly Change[],
  workload: Schemas["WorkloadRef"],
  routes: readonly string[],
): { added: Extract<Change, { op: "addDomain" }>[]; edits: Map<string, DomainEdit> } {
  const added: Extract<Change, { op: "addDomain" }>[] = []
  const edits = new Map<string, DomainEdit>()
  for (const change of changes)
    if (change.op === "addDomain") {
      if (workloadKey(change.workload) === workloadKey(workload)) added.push(change)
    } else if (change.op === "removeDomain" || change.op === "renameDomain") {
      if (!routes.includes(change.route.name)) continue
      const host = change.op === "removeDomain" ? change.hostname : change.from
      edits.set(`${change.route.name}/${host}`, change)
    }
  return { added, edits }
}
