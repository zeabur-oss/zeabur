import type { Schemas } from "@/api/client"
import { kubeAmount } from "@/lib/amount"
import { roleYaml } from "@/model/rbac"
import { bytesOf } from "@/model/volume-edit"
import { countsHealth, healthRank } from "@/model/service"

export type ProjectIconName = Schemas.ProjectIcon

/** The project label (zeabur-core `LABEL_PROJECT`). */
const PROJECT_LABEL = "zeabur.com/project"

/** In the picker's order (zeabur-model `ProjectIcon::ALL`). */
export const projectIcons: readonly ProjectIconName[] = [
  "folder",
  "globe",
  "storefront",
  "rocket",
  "cube",
  "database",
  "cloud",
  "code",
  "terminal",
  "chart",
  "chat",
  "book",
  "flask",
  "game",
  "heart",
  "briefcase",
]

export function projectTitle(
  project: Pick<Schemas.ProjectSummary, "name" | "displayName"> | undefined,
  name: string,
): string {
  return project?.displayName ?? name
}

/** A project named in a sentence: its title, and its namespace beside it when the two differ, so
 * every project in one sentence is named the same way. */
export function projectMention(
  project: Pick<Schemas.ProjectSummary, "name" | "displayName"> | undefined,
  name: string,
): { title: string; namespace?: string } {
  const title = projectTitle(project, name)
  return title === name ? { title } : { title, namespace: name }
}

export function otherNamespaces(
  items: readonly Schemas.ProjectSummary[],
  showSystem: boolean,
): { name: string; icon?: ProjectIconName | null; adoptable: boolean }[] {
  return items
    .filter((p) => !p.adopted && (showSystem || !p.system))
    .map((p) => ({ name: p.name, icon: p.icon, adoptable: !p.system }))
}

/** `unknown` offers it: the plan's dry run decides. */
export const mayChange = (namespace: Pick<Schemas.ProjectNamespace, "canChange">) =>
  namespace.canChange !== "denied"

export const mayAdopt = (namespace: Pick<Schemas.ProjectNamespace, "canAdopt">) =>
  namespace.canAdopt !== "denied"

/** zeabur-model `kernel::MANAGE_VERB`: what the chart's admission policy asks for before a label change. */
const MANAGE_VERB = "zeabur-manage"

export const namespaceOps = new Set<Schemas.Change["op"]>([
  "adopt",
  "unadopt",
  "deleteProject",
  "forceDeleteProject",
  "details",
  "buildLimits",
  "createProject",
])

const QUANTITY = /^\+?(\d+\.?\d*|\.\d+)(n|u|m|k|M|G|T|P|E|Ki|Mi|Gi|Ti|Pi|Ei|[eE][+-]?\d+)?$/

/** Whether Builder takes it as a build limit: a Kubernetes quantity above zero (zeabur-model
 * `is_build_quantity`). */
export function isBuildQuantity(value: string): boolean {
  const text = value.trim()
  return QUANTITY.test(text) && /[1-9]/.test(text.replace(/[eE][+-]?\d+$/, ""))
}

export type BuildLimit = "buildMemory" | "buildCpu"

/** The largest node's room, as the namespace says it; the plan checks it again. */
export function buildCeiling(
  namespace: Pick<Schemas.ProjectNamespace, "maxBuildMemoryBytes" | "maxBuildCpuMillicores">,
): { buildMemory?: string; buildCpu?: string } {
  const { maxBuildMemoryBytes: memory, maxBuildCpuMillicores: cpu } = namespace
  return {
    buildMemory:
      memory == null ? undefined : kubeAmount(Math.floor(memory / 2 ** 20) * 2 ** 20, "bytes"),
    buildCpu: cpu == null ? undefined : kubeAmount(cpu / 1000, "cpu"),
  }
}

/** What is wrong with a build limit as typed: not a quantity above zero, or more than the
 * largest node has. */
export function buildLimitProblem(
  kind: BuildLimit,
  value: string,
  namespace: Pick<Schemas.ProjectNamespace, "maxBuildMemoryBytes" | "maxBuildCpuMillicores">,
): "invalid" | "tooLarge" | undefined {
  const text = value.trim()
  if (text === "") return undefined
  if (!isBuildQuantity(text)) return "invalid"
  if (kind === "buildMemory") {
    const bytes = bytesOf(text)
    const max = namespace.maxBuildMemoryBytes
    return bytes !== undefined && max != null && bytes > max ? "tooLarge" : undefined
  }
  const match = /^\+?(\d+(?:\.\d*)?|\.\d+)(m)?$/.exec(text)
  const max = namespace.maxBuildCpuMillicores
  if (!match || max == null) return undefined
  return Number(match[1]) * (match[2] ? 1 : 1000) > max ? "tooLarge" : undefined
}

/** The plan change for the build limits a form holds; an empty field removes its annotation. */
export function buildLimitsChange(values: {
  buildMemory: string
  buildCpu: string
}): Extract<Schemas.Change, { op: "buildLimits" }> {
  return {
    op: "buildLimits",
    buildMemory: values.buildMemory.trim() || undefined,
    buildCpu: values.buildCpu.trim() || undefined,
  }
}

/** zeabur-model `DISPLAY_NAME_MAX`. */
export const DISPLAY_NAME_MAX = 64
const NAMESPACE_MAX = 63
const DNS_LABEL = /^[a-z0-9]([-a-z0-9]*[a-z0-9])?$/

/** Why a new project's name won't do, as zeabur-plan's `createProject` would refuse it. */
export function newProjectProblem(
  name: string,
  taken: readonly string[],
): "empty" | "long" | "invalid" | "system" | "taken" | undefined {
  if (name === "") return "empty"
  if (name.length > NAMESPACE_MAX) return "long"
  if (!DNS_LABEL.test(name)) return "invalid"
  if (name.startsWith("kube-")) return "system"
  return taken.includes(name) ? "taken" : undefined
}

export function kubectlLabel(namespace: string): { adopt: string; unadopt: string } {
  return {
    adopt: `kubectl label namespace ${namespace} ${PROJECT_LABEL}=true`,
    unadopt: `kubectl label namespace ${namespace} ${PROJECT_LABEL}-`,
  }
}

/** RBAC cannot narrow a patch to some labels, so this also covers the namespace's other labels
 * (https://kubernetes.io/docs/reference/access-authn-authz/rbac/#referring-to-resources), unless
 * the chart's admission policy guards them; `manage` lets it past the policy. */
export function patchRole(namespace: string, user: string, manage: boolean): string {
  return roleYaml({
    name: `zeabur-project-${namespace}`,
    rules: [
      {
        apiGroup: "",
        resources: ["namespaces"],
        resourceNames: [namespace],
        verbs: manage ? ["get", "patch", MANAGE_VERB] : ["get", "patch"],
      },
    ],
    username: user,
  })
}

type ProjectSummary = Schemas.ProjectSummary

/** Restarts summed over its services; absent when its workloads were not read. */
export const restartsIn = (project: ProjectSummary) =>
  project.services?.reduce((sum, service) => sum + service.restarts, 0)

/** Whether its name, display name, namespace or one of `hosts` holds `needle` (lower case). */
export function matchesProject(project: ProjectSummary, hosts: string[], needle: string): boolean {
  if (!needle) return true
  return [project.name, project.displayName ?? "", ...hosts]
    .join(" ")
    .toLowerCase()
    .includes(needle)
}

const healthOrder = (p: ProjectSummary) =>
  p.health ? healthRank[countsHealth(p.health)] : healthRank.ok
const named = (p: ProjectSummary) => p.displayName ?? p.name

/** Failing first, or by name; `descending` turns the order around, ties stay by name. */
export function compareProjects(
  a: ProjectSummary,
  b: ProjectSummary,
  sort: "health" | "name",
  descending: boolean,
): number {
  const sign = descending ? -1 : 1
  if (sort === "health") {
    return sign * (healthOrder(a) - healthOrder(b)) || named(a).localeCompare(named(b))
  }
  return sign * named(a).localeCompare(named(b))
}
