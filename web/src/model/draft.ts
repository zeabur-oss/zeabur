// Edits gathered on a service's Networking, Variables and Settings tabs before one plan: which
// part each belongs to, and how a later edit of the same thing replaces an earlier one.
import type { Schemas } from "@/api/client"
import type { DraftPart } from "@/routes/search"

type Change = Schemas["Change"]

const PARTS: Partial<Record<Change["op"], DraftPart>> = {
  env: "variables",
  data: "variables",
  addDomain: "networking",
  renameDomain: "networking",
  removeDomain: "networking",
  certificate: "networking",
  redirect: "networking",
  basicAuth: "networking",
  maintenance: "networking",
  errorPages: "networking",
  errorPageHtml: "networking",
  pageServer: "networking",
  addVolume: "storage",
  expandVolume: "storage",
  editMount: "storage",
  removeVolume: "storage",
  scale: "compute",
  resources: "compute",
  probe: "health-checks",
  editTask: "schedule",
}

export function partOf(change: Change): DraftPart | undefined {
  return PARTS[change.op]
}

/** The parts in the order of the tabs. */
export const partOrder: DraftPart[] = [
  "networking",
  "variables",
  "schedule",
  "compute",
  "health-checks",
  "storage",
]

/** Whether every change of an ask waits in the draft rather than being planned at once. */
export const draftable = (changes: readonly Change[]) =>
  changes.length > 0 && changes.every((c) => partOf(c) !== undefined)

const workloadKey = (w: Schemas["WorkloadRef"]) => `${w.kind}/${w.name}`

type Keys = { [Op in Change["op"]]?: (change: Extract<Change, { op: Op }>) => string }

const domainKey = (change: { hostname: string }) => `domain:${change.hostname}`
const hostKey = (change: { op: string; route: { name: string }; hostname: string }) =>
  `${change.op}:${change.route.name}/${change.hostname}`
const routeKey = (change: { op: string; route: { name: string } }) =>
  `${change.op}:${change.route.name}`
const mountKey = (change: { workload: Schemas["WorkloadRef"]; mountPath: string }) =>
  `mount:${workloadKey(change.workload)}:${change.mountPath}`

/** Two changes with one key edit the same thing: the later one stands. */
const keys: Keys = {
  env: (c) => `env:${workloadKey(c.workload)}:${c.container ?? ""}`,
  data: (c) => `data:${c.kind}/${c.name}`,
  scale: (c) => `scale:${workloadKey(c.workload)}`,
  resources: (c) => `resources:${workloadKey(c.workload)}:${c.container ?? ""}`,
  probe: (c) => `probe:${workloadKey(c.workload)}:${c.container ?? ""}:${c.probe}`,
  addDomain: domainKey,
  removeDomain: domainKey,
  renameDomain: (c) => `domain:${c.from}`,
  certificate: hostKey,
  redirect: hostKey,
  maintenance: hostKey,
  basicAuth: routeKey,
  errorPages: routeKey,
  errorPageHtml: (c) => c.op,
  pageServer: (c) => c.op,
  editTask: (c) => `editTask:${c.cronJob}`,
  expandVolume: (c) => `expand:${c.claim}`,
  editMount: mountKey,
  removeVolume: mountKey,
}

const keyOf = (change: Change) => (keys[change.op] as ((c: Change) => string) | undefined)?.(change)

/** A later edit of these keeps the fields it leaves out. */
const MERGED = new Set<Change["op"]>(["errorPageHtml", "pageServer", "editTask"])

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
  if (before.op === after.op && MERGED.has(after.op)) return { ...before, ...after } as Change
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

/** The staged changes of a domain's certificate, redirect, password and pages drawers that touch
 * `host` of `route`. A password and error pages are the whole route's. */
export function stagedSettings(changes: readonly Change[], route: string, host: string): Change[] {
  return changes.filter((change) => {
    switch (change.op) {
      case "certificate":
      case "redirect":
      case "maintenance":
        return change.route.name === route && change.hostname === host
      case "basicAuth":
      case "errorPages":
        return change.route.name === route
      default:
        return false
    }
  })
}
