import type { Schemas } from "@/api/client"
import { roleYaml } from "@/model/rbac"

export type ProjectIconName = Schemas["ProjectIcon"]

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
  project: Pick<Schemas["ProjectSummary"], "name" | "displayName"> | undefined,
  name: string,
): string {
  return project?.displayName ?? name
}

export function otherNamespaces(
  items: readonly Schemas["ProjectSummary"][],
  showSystem: boolean,
): { name: string; icon?: ProjectIconName | null; adoptable: boolean }[] {
  return items
    .filter((p) => !p.adopted && (showSystem || !p.system))
    .map((p) => ({ name: p.name, icon: p.icon, adoptable: !p.system }))
}

/** `unknown` offers it: the plan's dry run decides. */
export const mayChange = (namespace: Pick<Schemas["ProjectNamespace"], "canChange">) =>
  namespace.canChange !== "denied"

export const namespaceOps = new Set<Schemas["Change"]["op"]>(["adopt", "unadopt", "details"])

export function kubectlLabel(namespace: string): { adopt: string; unadopt: string } {
  return {
    adopt: `kubectl label namespace ${namespace} ${PROJECT_LABEL}=true`,
    unadopt: `kubectl label namespace ${namespace} ${PROJECT_LABEL}-`,
  }
}

/** RBAC cannot narrow a patch to some labels, so this also covers the namespace's other labels
 * (https://kubernetes.io/docs/reference/access-authn-authz/rbac/#referring-to-resources). */
export function patchRole(namespace: string, user: string): string {
  return roleYaml({
    name: `zeabur-project-${namespace}`,
    rules: [
      {
        apiGroup: "",
        resources: ["namespaces"],
        resourceNames: [namespace],
        verbs: ["get", "patch"],
      },
    ],
    username: user,
  })
}
