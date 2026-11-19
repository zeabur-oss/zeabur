import type { Schemas } from "@/api/client"

export type ClusterRights = Schemas.ClusterRights
type Permission = Schemas.Permission

/** The least product role that holds a right. */
export type NeededRole = "developer" | "admin"

const rules = {
  deploy: { role: "developer", rule: "create deployments.apps" },
  changeServices: { role: "developer", rule: "patch deployments.apps" },
  deleteServices: { role: "developer", rule: "delete deployments.apps" },
  scale: { role: "developer", rule: "patch deployments/scale.apps" },
  readSecrets: { role: "developer", rule: "get secrets" },
  changeSecrets: { role: "developer", rule: "patch secrets" },
  changeConfig: { role: "developer", rule: "patch configmaps" },
  changeDomains: { role: "developer", rule: "create ingresses.networking.k8s.io" },
  createServices: { role: "developer", rule: "create services" },
  changeStorage: { role: "developer", rule: "create persistentvolumeclaims" },
  changeDatabases: { role: "developer", rule: "create clusters.postgresql.cnpg.io" },
  changeObjectStorage: { role: "developer", rule: "create seaweeds.seaweed.seaweedfs.com" },
  backUpDatabases: { role: "developer", rule: "create backups.postgresql.cnpg.io" },
  backUpVolumes: { role: "developer", rule: "create schedules.k8up.io" },
  restoreVolumes: { role: "developer", rule: "create restores.k8up.io" },
  openTerminal: { role: "developer", rule: "create pods/exec" },
  portForward: { role: "developer", rule: "create pods/portforward" },
  readLogs: { role: "developer", rule: "get pods/log" },
  build: { role: "developer", rule: "patch builds.builder.zeabur.com" },
  deployFromGit: { role: "developer", rule: "create builds.builder.zeabur.com" },
  changeNetworkPolicies: { role: "developer", rule: "patch networkpolicies.networking.k8s.io" },
  changeMembers: { role: "admin", rule: "create rolebindings.rbac.authorization.k8s.io" },
  changeApiTokens: { role: "admin", rule: "create serviceaccounts -n zeabur-api-tokens" },
  changeNodes: { role: "admin", rule: "patch nodes" },
  changeBackups: { role: "admin", rule: "create schedules.velero.io" },
  restoreBackups: { role: "admin", rule: "create restores.velero.io" },
  changeDomainSuffixes: { role: "admin", rule: "patch configmaps/zeabur-settings" },
  changeNotifications: {
    role: "developer",
    rule: "create vmalertmanagerconfigs.operator.victoriametrics.com",
  },
  testNotifications: { role: "developer", rule: "create jobs.batch" },
  changeClusterNotifications: {
    role: "admin",
    rule: "create vmalertmanagerconfigs.operator.victoriametrics.com",
  },
  testClusterNotifications: { role: "admin", rule: "create jobs.batch" },
  changeS3Destinations: { role: "admin", rule: "create secrets -n zeabur-s3-destinations" },
  changeGitHubApp: { role: "admin", rule: "create secrets -n builder-system" },
  changeBuildLimits: { role: "admin", rule: "patch and zeabur-manage namespaces" },
} satisfies Record<string, { role: NeededRole; rule: string }>

export type Right = keyof typeof rules
/** Rights a page reads itself rather than from the project's or the cluster's review. */
type PageRight =
  | "changeNotifications"
  | "testNotifications"
  | "changeClusterNotifications"
  | "testClusterNotifications"
  | "changeDomainSuffixes"
  | "changeS3Destinations"
  | "changeGitHubApp"
  | "restoreBackups"
  | "createServices"
  | "changeBuildLimits"
  | "changeApiTokens"
/** A right in a project, closed when the project is read-only for the person. */
export type ProjectRight = Exclude<Right, keyof ClusterRights | PageRight>

export const neededRole = (right: Right): NeededRole => rules[right].role
export const ruleOf = (right: Right): string => rules[right].rule

/** Who is asking, as far as closing an action goes. */
export interface Asker {
  /** Signed in with Zeabur Identity, whose roles the interface can name. */
  member: boolean
}

export function askerOf(whoami: Pick<Schemas.WhoAmI, "username"> | undefined): Asker {
  return { member: whoami?.username.startsWith("zeabur:") ?? false }
}

/** An action is open, or closed with the role (for a member) or the rule (for anyone else) that
 * would open it. */
export type Gate =
  | { open: true }
  | { open: false; right: Right; role: NeededRole; rule: string; member: boolean }

const open: Gate = { open: true }

/** Only a definite denial closes an action: `unknown` (another authorizer may allow it) and a
 * right not yet read stay open, and the plan's dry run decides. */
export function gate(right: Right, permission: Permission | undefined, asker: Asker): Gate {
  if (permission !== "denied") return open
  return { open: false, right, ...rules[right], member: asker.member }
}

/** A project right's answer: denied when the project is read-only, unknown when that is not
 * known. */
export const projectPermission = (readOnly: boolean | null | undefined): Permission =>
  readOnly === true ? "denied" : readOnly === false ? "allowed" : "unknown"

const workloads = ["deployments", "statefulsets", "daemonsets"]

/** The right that changes (or deletes) objects of `resource`, when one covers it; any other kind
 * is left to the plan's dry run. */
export function rightOfResource(
  resource: string,
  change: "edit" | "delete",
): ProjectRight | undefined {
  if (workloads.includes(resource)) return change === "delete" ? "deleteServices" : "changeServices"
  const rights: Record<string, ProjectRight> = {
    secrets: "changeSecrets",
    configmaps: "changeConfig",
    ingresses: "changeDomains",
    httproutes: "changeDomains",
    persistentvolumeclaims: "changeStorage",
    networkpolicies: "changeNetworkPolicies",
    clusters: "changeDatabases",
    perconaservermongodbs: "changeDatabases",
    mysqlclusters: "changeDatabases",
    mariadbs: "changeDatabases",
    dragonflies: "changeDatabases",
    seaweeds: "changeObjectStorage",
    buckets: "changeObjectStorage",
  }
  return rights[resource]
}
