import type { Problem, Schemas } from "@/api/client"
import { isRefusal } from "@/model/problem"

export type Member = Schemas["MemberView"]
export type Passkey = Schemas["PasskeyView"]
export type Invitation = Schemas["InvitationView"]
export type SetupLinks = Schemas["SetupLinks"]

/** Above this many members the list is a table with a filter. */
const TABLE_FROM = 11

/** How the list is laid out: one line per member, or a filterable table. */
export function membersLayout(count: number): "empty" | "list" | "table" {
  if (count === 0) return "empty"
  return count >= TABLE_FROM ? "table" : "list"
}

/** You first, then admins, then everyone by name. */
export function sortMembers(members: readonly Member[]): Member[] {
  const rank = (m: Member) => (m.you ? 0 : m.role === "admin" ? 1 : 2)
  return members.toSorted((a, b) => rank(a) - rank(b) || a.username.localeCompare(b.username))
}

/** Members whose user name, display name or group holds every word of `query`. */
export function filterMembers(members: readonly Member[], query: string): Member[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean)
  if (words.length === 0) return [...members]
  return members.filter((m) => {
    const text = [m.username, m.displayName ?? "", ...m.groups].join(" ").toLowerCase()
    return words.every((word) => text.includes(word))
  })
}

/** One passkey is one lost device away from being locked out. */
export function needsBackup(member: Pick<Member, "passkeys">): boolean {
  return member.passkeys.length === 1
}

/** Why a passkey may not be removed by the person asking, or nothing when it may. Your own last
 * passkey stays, as the server also refuses. */
export function passkeyKept(member: Pick<Member, "you" | "passkeys">): "last" | undefined {
  return member.you && member.passkeys.length <= 1 ? "last" : undefined
}

/** Whether a member can be removed by the person asking: never yourself. */
export function canRemoveMember(member: Pick<Member, "you">): boolean {
  return !member.you
}

/** Whether setup links can be made, and whether they work once made. */
export function linkState(links: SetupLinks): "ready" | "no-issuer" | "not-redeemed" {
  if (!links.issuer) return "no-issuer"
  return links.accepted === false ? "not-redeemed" : "ready"
}

/** A Zeabur Identity user name, as its `User` schema takes it. */
export function isUsername(name: string): boolean {
  return (
    name.length > 0 &&
    name.length <= 63 &&
    name.split(".").every((label) => /^[a-z0-9]([a-z0-9-]*[a-z0-9])?$/.test(label))
  )
}

/** What typing a user name into the invitation leads to. */
export function inviteTarget(
  members: readonly Member[],
  username: string,
): "invalid" | "new" | "existing" {
  const name = username.trim()
  if (!isUsername(name)) return "invalid"
  return members.some((m) => m.username === name) ? "existing" : "new"
}

/** What a reset or a removal takes away from a member, for its confirmation. */
export function losses(member: Member): { passkeys: number; linked: number; sessions: number } {
  return {
    passkeys: member.passkeys.length,
    linked: member.linked.length,
    sessions: member.sessions,
  }
}

/** The name a member goes by: their display name, else their user name. */
export function memberName(member: Pick<Member, "username" | "displayName">): string {
  return member.displayName?.trim() || member.username
}

export type ProjectRole = Schemas["ProjectRole"]
export type ProjectGrant = Schemas["ProjectGrant"]

/** Up to this many projects are named in a role; more become a count. */
const NAMED_UP_TO = 3

/** A member's rights at a glance: admin, a role per project, none, or not known (the member roles
 * are off or their bindings could not be read). Developer comes before Viewer. */
export type AccessSummary =
  | { kind: "admin" }
  | { kind: "unknown" }
  | { kind: "none" }
  | {
      kind: "projects"
      roles: { role: ProjectRole; named: string[]; count: number }[]
    }

export function accessSummary(member: Pick<Member, "role" | "projects">): AccessSummary {
  if (member.role === "admin") return { kind: "admin" }
  const projects = member.projects
  if (projects == null) return { kind: "unknown" }
  if (projects.length === 0) return { kind: "none" }
  const roles = (["developer", "viewer"] as const).flatMap((role) => {
    const names = projects
      .filter((p) => p.role === role)
      .map((p) => p.project)
      .toSorted()
    if (names.length === 0) return []
    return [{ role, named: names.length <= NAMED_UP_TO ? names : [], count: names.length }]
  })
  return { kind: "projects", roles }
}

/** The projects a role can be given in, by name: every namespace but Kubernetes' own, and those
 * the member already has a role in, so a role in a project since deleted can still be taken away. */
export function grantableProjects(
  projects: readonly { name: string; system: boolean }[],
  held: Iterable<string> = [],
): string[] {
  const names = new Set(projects.filter((p) => !p.system).map((p) => p.name))
  for (const name of held) names.add(name)
  return [...names].toSorted()
}

/** Above this many projects the picker gets a filter. */
const PICKER_FILTER_FROM = 8

export function pickerHasFilter(count: number): boolean {
  return count >= PICKER_FILTER_FROM
}

/** Projects whose name holds `query`, and every chosen one, so a filter never hides a choice. Those
 * in `first` (the roles held when the picker opened) lead, so a held role is seen without
 * scrolling and a click never moves a row. */
export function pickerProjects(
  names: readonly string[],
  chosen: ReadonlyMap<string, ProjectRole>,
  query: string,
  first: ReadonlySet<string> = new Set(),
): string[] {
  const q = query.trim().toLowerCase()
  return names
    .filter((name) => chosen.has(name) || name.includes(q))
    .toSorted((a, b) => Number(first.has(b)) - Number(first.has(a)))
}

/** The request for a choice of projects, in name order. */
export function grantsOf(chosen: ReadonlyMap<string, ProjectRole>): ProjectGrant[] {
  return [...chosen]
    .map(([project, role]) => ({ project, role }))
    .toSorted((a, b) => a.project.localeCompare(b.project))
}

/** A member's current roles, to start a change from. */
export function chosenOf(member: Pick<Member, "projects">): Map<string, ProjectRole> {
  return new Map((member.projects ?? []).map((p) => [p.project, p.role]))
}

/** What a change of access takes away, for its warning: admin rights, every role in `removed`, and
 * the changes in `lowered` (Developer to Viewer). */
export function accessLosses(
  member: Pick<Member, "role" | "projects">,
  admin: boolean,
  chosen: ReadonlyMap<string, ProjectRole>,
): { admin: boolean; removed: string[]; lowered: string[] } {
  const held = admin ? [] : (member.projects ?? [])
  return {
    admin: member.role === "admin" && !admin,
    removed: held.filter((p) => !chosen.has(p.project)).map((p) => p.project),
    lowered: held
      .filter((p) => p.role === "developer" && chosen.get(p.project) === "viewer")
      .map((p) => p.project),
  }
}

/** Whether a member's access can be changed from what is known: not when their roles could not be
 * read, since saving would replace roles nobody saw. */
export function canChangeAccess(
  member: Pick<Member, "you" | "role" | "projects">,
  projectRoles: boolean,
): boolean {
  if (member.you) return false
  return !projectRoles || member.role === "admin" || member.projects != null
}

/** Whether a refused project is one a Zeabur Identity member has no role in: they are told an
 * admin can give them one. An admin, or someone signed in with a kubeconfig, sees the refusal
 * and its rule instead. */
export function lacksProjectRole(problem: Problem, whoami: Schemas["WhoAmI"] | undefined): boolean {
  if (!whoami?.username.startsWith("zeabur:")) return false
  if (whoami.groups.includes("zeabur:admins")) return false
  return isRefusal(problem)
}

/** Pending invitations still worth showing at `now`. */
export function pendingInvitations(invitations: readonly Invitation[], now: number): Invitation[] {
  return invitations.filter((i) => Date.parse(i.expiresAt) > now)
}
