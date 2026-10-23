import type { Schemas } from "@/api/client"

export type ProjectRights = Schemas["ProjectRights"]
export type ClusterRights = Schemas["ClusterRights"]
export type Right = keyof ProjectRights | keyof ClusterRights
type Permission = Schemas["Permission"]

/** The least product role that holds a right. */
export type NeededRole = "developer" | "admin"

const rules: Record<Right, { role: NeededRole; rule: string }> = {
  deploy: { role: "developer", rule: "create deployments.apps" },
  changeServices: { role: "developer", rule: "patch deployments.apps" },
  deleteServices: { role: "developer", rule: "delete deployments.apps" },
  scale: { role: "developer", rule: "patch deployments/scale.apps" },
  readSecrets: { role: "developer", rule: "get secrets" },
  changeSecrets: { role: "developer", rule: "patch secrets" },
  changeConfig: { role: "developer", rule: "patch configmaps" },
  changeDomains: { role: "developer", rule: "create ingresses.networking.k8s.io" },
  changeStorage: { role: "developer", rule: "create persistentvolumeclaims" },
  changeDatabases: { role: "developer", rule: "create clusters.postgresql.cnpg.io" },
  backUpDatabases: { role: "developer", rule: "create backups.postgresql.cnpg.io" },
  openTerminal: { role: "developer", rule: "create pods/exec" },
  readLogs: { role: "developer", rule: "get pods/log" },
  build: { role: "developer", rule: "patch builds.builder.zeabur.com" },
  deployFromGit: { role: "developer", rule: "create builds.builder.zeabur.com" },
  changeNetworkPolicies: { role: "developer", rule: "patch networkpolicies.networking.k8s.io" },
  changeMembers: { role: "admin", rule: "create rolebindings.rbac.authorization.k8s.io" },
  installComponents: { role: "admin", rule: "create helmcharts.helm.cattle.io" },
  changeNodes: { role: "admin", rule: "patch nodes" },
  changeBackups: { role: "admin", rule: "create schedules.velero.io" },
}

export const neededRole = (right: Right): NeededRole => rules[right].role
export const ruleOf = (right: Right): string => rules[right].rule

/** Who is asking, as far as closing an action goes. */
export interface Asker {
  /** Signed in with Zeabur Identity, whose roles the interface can name. */
  member: boolean
  /** In `zeabur:admins`. */
  admin: boolean
  elevation: Schemas["ElevationState"] | undefined
}

export function askerOf(
  whoami: Pick<Schemas["WhoAmI"], "username" | "groups"> | undefined,
  elevation: Schemas["ElevationState"] | undefined,
): Asker {
  return {
    member: whoami?.username.startsWith("zeabur:") ?? false,
    admin: whoami?.groups.includes("zeabur:admins") ?? false,
    elevation,
  }
}

/** Cluster administrators, and admins who may take admin rights: what only they may do (a node
 * terminal, the cluster's kubectl terminal) is offered, and its plan offers the rights. */
export function mayActAsClusterAdmin(clusterAdmin: boolean | undefined, asker: Asker): boolean {
  return clusterAdmin === true || (asker.admin && asker.elevation === "available")
}

/** An action is open, or closed with the role (for a member) or the rule (for anyone else) that
 * would open it. */
export type Gate =
  | { open: true }
  | { open: false; right: Right; role: NeededRole; rule: string; member: boolean }

const open: Gate = { open: true }

/** Only a definite denial closes an action: `unknown` (another authorizer may allow it) and a
 * right not yet read stay open, and the plan's dry run decides. An admin who may take admin
 * rights keeps the action open too; the plan offers them. */
export function gate(right: Right, permission: Permission | undefined, asker: Asker): Gate {
  if (permission !== "denied") return open
  if (asker.admin && asker.elevation === "available") return open
  return { open: false, right, ...rules[right], member: asker.member }
}

/** Every right `unknown`: for tests and for a project whose review failed. */
export function unknownRights(): ProjectRights {
  return Object.fromEntries(
    (Object.keys(rules) as Right[])
      .filter((r) => !(r in clusterOnly))
      .map((r) => [r, "unknown" as const]),
  ) as ProjectRights
}

const clusterOnly: Record<keyof ClusterRights, true> = {
  installComponents: true,
  changeNodes: true,
  changeBackups: true,
}

const workloads = ["deployments", "statefulsets", "daemonsets"]

/** The right that changes (or deletes) objects of `resource`, when one covers it; any other kind
 * is left to the plan's dry run. */
export function rightOfResource(
  resource: string,
  change: "edit" | "delete",
): keyof ProjectRights | undefined {
  if (workloads.includes(resource)) return change === "delete" ? "deleteServices" : "changeServices"
  const rights: Record<string, keyof ProjectRights> = {
    secrets: "changeSecrets",
    configmaps: "changeConfig",
    ingresses: "changeDomains",
    httproutes: "changeDomains",
    persistentvolumeclaims: "changeStorage",
    networkpolicies: "changeNetworkPolicies",
    clusters: "changeDatabases",
    perconaservermongodbs: "changeDatabases",
  }
  return rights[resource]
}
