import type { Schemas } from "@/api/client"

export type ClusterRights = Schemas.ClusterRights
type Permission = Schemas.Permission

export type Right = Schemas.Right
type Rights = Schemas.Rights

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

/** Who is asking, as far as closing an action goes. */
export interface Asker {
  /** Signed in with Zeabur Identity, whose roles the interface can name. */
  member: boolean
}

export function askerOf(whoami: Pick<Schemas.WhoAmI, "member"> | undefined): Asker {
  return { member: whoami?.member ?? false }
}

/** An action is open, or closed for the person; the server's rights say the role (for a member)
 * or the rule (for anyone else) that would open it. */
export type Gate = { open: true } | { open: false; right: Right; member: boolean }

const open: Gate = { open: true }

/** Only a definite denial closes an action: `unknown` (another authorizer may allow it) and a
 * right not yet read stay open, and the plan's dry run decides. */
export function gate(right: Right, permission: Permission | undefined, asker: Asker): Gate {
  if (permission !== "denied") return open
  return { open: false, right, member: asker.member }
}

/** A project right's answer: denied when the project is read-only, unknown when that is not
 * known. */
export const projectPermission = (readOnly: boolean | null | undefined): Permission =>
  readOnly === true ? "denied" : readOnly === false ? "allowed" : "unknown"

/** What holds `right`, from the server's rights. */
export const needsOf = (rights: Rights | undefined, right: Right) =>
  rights?.rights.find((r) => r.right === right)

/** The right that changes (or deletes) objects of `resource`, when one covers it; any other kind
 * is left to the plan's dry run. */
export function rightOfResource(
  rights: Rights | undefined,
  resource: string,
  change: "edit" | "delete",
): ProjectRight | undefined {
  const covered = rights?.resources.find((r) => r.resource === resource)
  return covered?.[change] as ProjectRight | undefined
}
