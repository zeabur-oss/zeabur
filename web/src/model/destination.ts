// Where to look at what an applied plan changed.
import type { Schemas } from "@/api/client"

type Change = Schemas.Change
type WorkloadRef = Schemas.WorkloadRef
type Plan = Schemas.PlanResponse

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value)
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

/** Where a change to the project itself leads, by the project's encoded name. */
const projectPlace: Partial<Record<Change["op"], (project: string) => string>> = {
  adopt: (project) => `/projects/${project}`,
  createProject: (project) => `/projects/${project}`,
  unadopt: () => "/projects",
  deleteProject: () => "/projects",
  forceDeleteProject: () => "/projects",
  details: (project) => `/projects/${project}/settings`,
  buildLimits: (project) => `/projects/${project}/settings`,
}

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
  const object = () => objectLink(project, plan)
  const own = workloadTab[change.op]
  if (own && "workload" in change && change.workload?.kind !== "Cluster")
    return tab(change.workload ?? undefined, own)
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
