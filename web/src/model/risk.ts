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
//   nothing in); opening a port to the internet; deleting an object, which cannot be taken back
//   and is confirmed by typing its name; deleting a project, confirmed by typing the project's.
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
  | "opensPort"
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

type OneWeight = { risk: Risk; reason?: Reason }

/** The weight of a change that does not depend on its fields. */
const fixedWeight: Partial<Record<Change["op"], OneWeight>> = {
  rollback: { risk: "high", reason: "rollback" },
  expandVolume: { risk: "high", reason: "storage" },
  addVolume: { risk: "high", reason: "storage" },
  editMount: { risk: "high", reason: "storage" },
  removeVolume: { risk: "high", reason: "storage" },
  swapClaim: { risk: "high", reason: "storage" },
  edit: { risk: "high", reason: "yamlEdit" },
  removeDomain: { risk: "high", reason: "removesDomain" },
  delete: { risk: "high", reason: "deletes" },
  deleteProject: { risk: "high", reason: "deletes" },
  image: { risk: "normal" },
  autoUpdate: { risk: "normal" },
  env: { risk: "normal" },
  addDomain: { risk: "normal" },
  renameDomain: { risk: "normal" },
  probe: { risk: "normal" },
  resources: { risk: "normal" },
  configReload: { risk: "normal" },
  unadopt: { risk: "normal" },
  debugContainer: { risk: "normal" },
  clusterTools: { risk: "normal" },
  securityProfile: { risk: "normal" },
  deployKey: { risk: "normal" },
  adopt: { risk: "low" },
  createProject: { risk: "low" },
  details: { risk: "low" },
  rebuild: { risk: "low" },
  uploadToken: { risk: "low" },
  archiveDigest: { risk: "normal" },
}

function weighOne(change: Change, podsOf: PodsOf): OneWeight {
  const fixed = fixedWeight[change.op]
  if (fixed) return fixed
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
    case "create":
      return isolates(change.object) ? { risk: "high", reason: "isolates" } : { risk: "normal" }
    default:
      return weighAccess(change as AccessChange | ServiceChange)
  }
}

function weighAccess(change: AccessChange | ServiceChange): { risk: Risk; reason?: Reason } {
  if (change.op === "egressLimit" && change.enabled) return { risk: "high", reason: "isolates" }
  return change.op === "publicPort" && !change.remove
    ? { risk: "high", reason: "opensPort" }
    : { risk: "normal" }
}

/** Changes to a service that only open a port can make riskier, copied S3 keys and templates. */
type ServiceChange = Extract<
  Change,
  {
    op:
      | "buildOptions"
      | "uploadToken"
      | "archiveDigest"
      | "labelVolume"
      | "errorPageHtml"
      | "pageServer"
      | "template"
      | "errorPages"
      | "maintenance"
      | "copyS3Keys"
  }
>

/** The changes of scheduled tasks, routes, shared variables and public ports. */
export type AccessChange = Extract<
  Change,
  {
    op:
      | "scheduleTask"
      | "runTask"
      | "editTask"
      | "redirect"
      | "basicAuth"
      | "ipAllow"
      | "egressLimit"
      | "certificate"
      | "envImport"
      | "publicPort"
  }
>

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

/** The name a person types to confirm a plan that deletes something: the project's when it
 * deletes `project`, else the first object it deletes; `undefined` when there is none. */
export function typedName(changes: readonly Change[], project?: string): string | undefined {
  for (const change of changes) {
    if (change.op === "deleteProject" && project !== undefined) return project
    if (change.op === "delete") return change.name
  }
  return undefined
}

/** Whether a plan still waits for its name to be typed: it deletes something, and `typed` is
 * not that name (surrounding spaces aside; case counts, as Kubernetes names are lower case). */
export function awaitsName(changes: readonly Change[], typed: string, project?: string): boolean {
  const name = typedName(changes, project)
  return name !== undefined && typed.trim() !== name
}

const NAME_MARK = "\u0000"

/** The text before and after the name in the sentence `say` makes around one. */
export function aroundName(say: (name: string) => string): [string, string] {
  const [before = "", after = ""] = say(NAME_MARK).split(NAME_MARK)
  return [before, after]
}

/** The service page tab a change is seen on. */
type Tab =
  | "variables"
  | "networking"
  | "deployments"
  | "settings#compute"
  | "settings#health-checks"
  | "settings#storage"

/** The service tab of a change to one workload. */
const workloadTab: Partial<Record<Change["op"], Tab>> = {
  restart: "deployments",
  rollback: "deployments",
  env: "variables",
  scale: "settings#compute",
  resources: "settings#compute",
  configReload: "settings#compute",
  probe: "settings#health-checks",
  addDomain: "networking",
  addVolume: "settings#storage",
  editMount: "settings#storage",
  removeVolume: "settings#storage",
  swapClaim: "settings#storage",
}

const isNetworkPolicy = (object: unknown) => isRecord(object) && object.kind === "NetworkPolicy"

/** The Objects page at the object the plan's first write addresses. */
function objectLink(project: string, plan: Pick<Plan, "objects"> | undefined): string | undefined {
  const target = plan?.objects[0]?.target
  if (!target) return undefined
  const [group, version] = target.apiVersion.includes("/")
    ? target.apiVersion.split("/")
    : ["core", target.apiVersion]
  const key = [group, version, target.plural, target.name].join("/")
  return `/projects/${encodeURIComponent(project)}/objects?object=${encodeURIComponent(key)}`
}

/** Where to look at what a plan changed, as a path and query: the service page tab for a
 * workload's change, the object on the Objects page for anything else. `service` is the service
 * the change was asked from, for changes that name only a route, a claim or an object. */
/** Where a change to the project itself leads, by the project's encoded name. */
const projectPlace: Partial<Record<Change["op"], (project: string) => string>> = {
  adopt: (project) => `/projects/${project}`,
  createProject: (project) => `/projects/${project}`,
  unadopt: () => "/projects",
  deleteProject: () => "/projects",
  details: (project) => `/projects/${project}/settings`,
}

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
  const object = () => objectLink(project, plan)
  const own = workloadTab[change.op]
  if (own && "workload" in change) return tab(change.workload ?? undefined, own)
  const place = projectPlace[change.op]
  if (place) return place(encodeURIComponent(project))
  switch (change.op) {
    case "renameDomain":
    case "removeDomain":
      return tab(service, "networking")
    case "expandVolume":
      return tab(service, "settings#storage") ?? object()
    case "create":
      return isNetworkPolicy(change.object) && service ? tab(service, "networking") : object()
    case "data":
    case "edit":
      return object()
    case "delete":
      // The object is gone; a policy's effect shows on the service it was asked from.
      return change.resource.resource === "networkpolicies" ? tab(service, "networking") : undefined
    default:
      return tab(...accessTab(change, service))
  }
}

/** No workload where the change has no tab of its own. */
function accessTab(change: Change, service?: WorkloadRef): [WorkloadRef | undefined, Tab] {
  switch (change.op) {
    case "envImport":
      return [change.workload, "variables"]
    case "basicAuth":
    case "errorPages":
    case "maintenance":
    case "certificate":
    case "redirect":
      return [service, "networking"]
    case "publicPort":
      return [change.workload, "networking"]
    default:
      return [undefined, "networking"]
  }
}
