import type { Schemas } from "@/api/client"

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

/** Pending invitations still worth showing at `now`. */
export function pendingInvitations(invitations: readonly Invitation[], now: number): Invitation[] {
  return invitations.filter((i) => Date.parse(i.expiresAt) > now)
}
