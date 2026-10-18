// A project as the shell names it: its display name when it has one, else its namespace's name,
// and its icon. Both come with the project list (the namespace's annotations), so naming a
// project reads no project document. Also the help for someone who may not patch a namespace:
// the RBAC to ask for and the kubectl command that does the same as engawa.
import type { Schemas } from "@/api/client"
import { roleYaml } from "@/model/rbac"

export type ProjectIconName = Schemas["ProjectIcon"]

/** The project label (engawa-core `LABEL_PROJECT`). */
const PROJECT_LABEL = "engawa.engawahq.dev/project"

/** Every built-in icon, in the order the picker shows them (engawa-model `ProjectIcon::ALL`). */
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

/** What names a project: its display name, else its namespace's name. */
export function projectTitle(
  project: Pick<Schemas["ProjectSummary"], "name" | "displayName"> | undefined,
  name: string,
): string {
  return project?.displayName ?? name
}

/** The namespaces that are not projects, for the list that offers "Adopt": the system ones only
 * when asked for, and those never with the button. */
export function otherNamespaces(
  items: readonly Schemas["ProjectSummary"][],
  showSystem: boolean,
): { name: string; icon?: ProjectIconName | null; adoptable: boolean }[] {
  return items
    .filter((p) => !p.adopted && (showSystem || !p.system))
    .map((p) => ({ name: p.name, icon: p.icon, adoptable: !p.system }))
}

/** Whether to offer changing the namespace: unless its rules say no. When they cannot tell
 * (`unknown`), the plan's dry run decides. */
export const mayChange = (namespace: Pick<Schemas["ProjectNamespace"], "canChange">) =>
  namespace.canChange !== "denied"

/** The changes that write the project's Namespace, not objects in it. */
export const namespaceOps = new Set<Schemas["Change"]["op"]>(["adopt", "unadopt", "details"])

/** The kubectl commands that adopt `namespace` and let it go, as engawa's plan does. */
export function kubectlLabel(namespace: string): { adopt: string; unadopt: string } {
  return {
    adopt: `kubectl label namespace ${namespace} ${PROJECT_LABEL}=true`,
    unadopt: `kubectl label namespace ${namespace} ${PROJECT_LABEL}-`,
  }
}

/** A ClusterRole that lets `user` patch only `namespace` (RBAC `resourceNames`), and its binding.
 * RBAC cannot narrow a patch to some labels, so this right also covers the namespace's other
 * labels (https://kubernetes.io/docs/reference/access-authn-authz/rbac/#referring-to-resources). */
export function patchRole(namespace: string, user: string): string {
  return roleYaml({
    name: `engawa-project-${namespace}`,
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
