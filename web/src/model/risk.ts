// How much a planned change can hurt, which decides how it is confirmed, and what undoes it.
//
// Every write is planned and applied the same way; only the confirmation changes with the risk.
// A change that is easy to take back and keeps the service up is confirmed lightly, a change
// that loses data, takes the service down or cannot be reversed is confirmed in full with its
// consequence first. The approach follows the guidance to make confirmations rare and specific
// so they keep their weight (https://www.nngroup.com/articles/confirmation-dialog/), and to offer
// Undo where an action can be reversed instead of asking first
// (https://www.nngroup.com/articles/user-control-and-freedom/).
//
// Rules, per change (a plan of several changes takes the highest):
// - low: restarting a workload with at least two ready pods (Kubernetes replaces them one by one
//   and the rest keep serving); scaling to one pod or more.
// - high, with the reason the confirmation spells out: a restart with fewer than two ready pods;
//   scaling to zero; deleting a ConfigMap or Secret key; a rollback; any storage change (a claim
//   cannot shrink, a new or moved mount replaces the pods); a YAML edit (written as typed);
//   removing a domain; a NetworkPolicy that isolates its pods (limits what they send, or lets
//   nothing in); deleting an object, which cannot be taken back and is confirmed by typing its
//   name. Revoking a join token (a bootstrap token Secret in kube-system) is not typed: it only
//   takes away access, and a node that has joined keeps working.
// - normal: everything else (variables, values set, domains added or renamed, resources, health
//   checks, other created objects, letting a namespace go).
// - low as well: adopting a namespace and a project's details, which touch only the Namespace's
//   label or annotations.
//
// Undo is offered only where the inverse is a plain change of its own: scaling back to the
// number of pods there were, and deleting an object just created, at the resourceVersion its
// creation returned (so an object someone changed since is kept). It is planned and applied
// again like any change.
import type { Schemas } from "@/api/client"

type Change = Schemas["Change"]
type WorkloadRef = Schemas["WorkloadRef"]
type Plan = Schemas["PlanResponse"]
type Applied = Schemas["Applied"]

export type Risk = "low" | "normal" | "high"

/** Why a change is high risk: the confirmation states the consequence for each. */
export type Reason =
  | "fewPods"
  | "scaleToZero"
  | "deletesKeys"
  | "rollback"
  | "storage"
  | "yamlEdit"
  | "removesDomain"
  | "isolates"
  | "deletes"

/** A workload's pods as the project document last reported them. */
export interface Pods {
  desired: number
  ready: number
}

/** Looks up a workload's pods; `undefined` when the page does not know them. */
export type PodsOf = (workload: WorkloadRef) => Pods | undefined

export interface Weight {
  risk: Risk
  /** The reasons of a high risk, in the order of the changes; empty otherwise. */
  reasons: Reason[]
}

const order: Record<Risk, number> = { low: 0, normal: 1, high: 2 }

/** The risk of a plan's changes: the highest of them. An empty list is normal. */
export function weigh(changes: readonly Change[], podsOf: PodsOf): Weight {
  if (changes.length === 0) return { risk: "normal", reasons: [] }
  let risk: Risk = "low"
  const reasons: Reason[] = []
  for (const change of changes) {
    const one = weighOne(change, podsOf)
    if (order[one.risk] > order[risk]) risk = one.risk
    if (one.reason && !reasons.includes(one.reason)) reasons.push(one.reason)
  }
  return { risk, reasons }
}

function weighOne(change: Change, podsOf: PodsOf): { risk: Risk; reason?: Reason } {
  switch (change.op) {
    case "restart":
      return (podsOf(change.workload)?.ready ?? 0) >= 2
        ? { risk: "low" }
        : { risk: "high", reason: "fewPods" }
    case "scale":
      return change.replicas === 0 ? { risk: "high", reason: "scaleToZero" } : { risk: "low" }
    case "data":
      return change.changes.some((edit) => edit.op === "remove")
        ? { risk: "high", reason: "deletesKeys" }
        : { risk: "normal" }
    case "rollback":
      return { risk: "high", reason: "rollback" }
    case "expandVolume":
    case "addVolume":
    case "editMount":
    case "removeVolume":
      return { risk: "high", reason: "storage" }
    case "edit":
      return { risk: "high", reason: "yamlEdit" }
    case "removeDomain":
      return { risk: "high", reason: "removesDomain" }
    case "create":
      return isolates(change.object) ? { risk: "high", reason: "isolates" } : { risk: "normal" }
    case "delete":
      return { risk: "high", reason: "deletes" }
    case "image":
    case "autoUpdate":
    case "env":
    case "addDomain":
    case "renameDomain":
    case "probe":
    case "resources":
    case "configReload":
    case "unadopt":
    case "debugContainer":
    case "clusterTools":
      return { risk: "normal" }
    case "adopt":
    case "createProject":
    case "details":
    case "rebuild":
      return { risk: "low" }
    case "deployKey":
      return { risk: "normal" }
  }
}

/** Whether an object is a NetworkPolicy that shuts its pods off: it limits what they send
 * (`Egress`), or it covers ingress and lets nothing in. Once it exists, only what policies allow
 * reaches them or leaves them. A policy that only admits more (an allow rule) is not: Kubernetes
 * adds the rules of every policy that selects a pod. */
export function isolates(object: unknown): boolean {
  if (!isRecord(object) || object.kind !== "NetworkPolicy") return false
  const spec = isRecord(object.spec) ? object.spec : {}
  const types = Array.isArray(spec.policyTypes) ? spec.policyTypes : undefined
  // Without policyTypes a policy covers ingress, and egress when it has egress rules.
  const egress = types ? types.includes("Egress") : Array.isArray(spec.egress)
  const ingress = types === undefined || types.includes("Ingress")
  const rules = Array.isArray(spec.ingress) ? spec.ingress : []
  return egress || (ingress && rules.length === 0)
}

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
    if (change.op === "create") {
      const object: Record<string, unknown> = isRecord(change.object) ? change.object : {}
      const name = isRecord(object.metadata) ? object.metadata.name : undefined
      const created = applied.find((a) => a.target.kind === object.kind && a.target.name === name)
      const deletion = created
        ? deletionOf({ ...created.target, resourceVersion: created.resourceVersion })
        : undefined
      if (!deletion) return undefined
      undo.push(deletion)
      continue
    }
    if (change.op !== "scale") return undefined
    const before = podsOf(change.workload)?.desired
    if (before === undefined || before === change.replicas) return undefined
    undo.push({ op: "scale", workload: change.workload, replicas: before })
  }
  return undo
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

/** Whether a change revokes a join token: it deletes a `bootstrap-token-<id>` Secret in
 * kube-system. */
export function revokesJoinToken(project: string, change: Change): boolean {
  return (
    project === "kube-system" &&
    change.op === "delete" &&
    change.resource.group === "" &&
    change.resource.resource === "secrets" &&
    /^bootstrap-token-[a-z0-9]{6}$/.test(change.name)
  )
}

/** The name a person types to confirm a plan that deletes something in `project`: the first
 * object it deletes, join tokens left out; `undefined` when there is none. */
export function typedName(changes: readonly Change[], project: string): string | undefined {
  for (const change of changes) {
    if (change.op === "delete" && !revokesJoinToken(project, change)) return change.name
  }
  return undefined
}

/** Whether a plan still waits for its name to be typed: it deletes something, and `typed` is
 * not that name (surrounding spaces aside; case counts, as Kubernetes names are lower case). */
export function awaitsName(changes: readonly Change[], project: string, typed: string): boolean {
  const name = typedName(changes, project)
  return name !== undefined && typed.trim() !== name
}

/** The service page tab a change is seen on. */
type Tab =
  | "variables"
  | "networking"
  | "deployments"
  | "settings#compute"
  | "settings#health-checks"
  | "settings#storage"

/** Where to look at what a plan changed, as a path and query: the service page tab for a
 * workload's change, the object on the Objects page for anything else. `service` is the service
 * the change was asked from, for changes that name only a route, a claim or an object. */
export function destinationOf(
  project: string,
  changes: readonly Change[],
  plan: Pick<Plan, "objects"> | undefined,
  service?: WorkloadRef,
): string | undefined {
  const [change] = changes
  if (!change) return undefined
  const tab = (workload: WorkloadRef | undefined, name: Tab) =>
    workload
      ? `/projects/${encodeURIComponent(project)}/services/${workload.kind}/${encodeURIComponent(workload.name)}?tab=${name}`
      : undefined
  const object = () => {
    const target = plan?.objects[0]?.target
    if (!target) return undefined
    const [group, version] = target.apiVersion.includes("/")
      ? target.apiVersion.split("/")
      : ["core", target.apiVersion]
    const key = [group, version, target.plural, target.name].join("/")
    return `/projects/${encodeURIComponent(project)}/objects?object=${encodeURIComponent(key)}`
  }
  switch (change.op) {
    case "restart":
    case "rollback":
      return tab(change.workload, "deployments")
    case "env":
      return tab(change.workload, "variables")
    case "scale":
    case "resources":
    case "configReload":
      return tab(change.workload, "settings#compute")
    case "probe":
      return tab(change.workload, "settings#health-checks")
    case "addDomain":
      return tab(change.workload, "networking")
    case "renameDomain":
    case "removeDomain":
      return tab(service, "networking")
    case "addVolume":
    case "editMount":
    case "removeVolume":
      return tab(change.workload, "settings#storage")
    case "expandVolume":
      return tab(service, "settings#storage") ?? object()
    case "create":
      return isRecord(change.object) && change.object.kind === "NetworkPolicy" && service
        ? tab(service, "networking")
        : object()
    case "data":
    case "edit":
      return object()
    case "delete":
      // The object is gone; a policy's effect shows on the service it was asked from.
      return change.resource.resource === "networkpolicies" ? tab(service, "networking") : undefined
    case "adopt":
    case "createProject":
      return `/projects/${encodeURIComponent(project)}`
    case "unadopt":
      return "/projects"
    case "details":
      return `/projects/${encodeURIComponent(project)}/settings`
  }
}
