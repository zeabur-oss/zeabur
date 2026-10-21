import { describe, expect, it } from "bun:test"

import {
  canRemoveMember,
  filterMembers,
  inviteTarget,
  isUsername,
  linkState,
  losses,
  memberName,
  membersLayout,
  needsBackup,
  passkeyKept,
  pendingInvitations,
  sortMembers,
  type Member,
  type Passkey,
} from "@/model/members"

const origin = { apiVersion: "identity.zeabur.com/v1alpha1", kind: "Credential", name: "c" }

const passkey = (id: string): Passkey => ({ id, synced: false, origin })

const member = (username: string, patch: Partial<Member> = {}): Member => ({
  id: `members/${username}`,
  username,
  groups: [],
  role: "member",
  disabled: false,
  passkeys: [passkey(`${username}-1`)],
  linked: [],
  sessions: 0,
  invitations: 0,
  you: false,
  origin: { ...origin, kind: "User", name: username },
  ...patch,
})

describe("members", () => {
  it("lays out none, a few as lines, and many as a table", () => {
    expect(membersLayout(0)).toBe("empty")
    expect(membersLayout(1)).toBe("list")
    expect(membersLayout(10)).toBe("list")
    expect(membersLayout(11)).toBe("table")
    expect(membersLayout(500)).toBe("table")
  })

  it("puts you first, then admins, then everyone by name", () => {
    const sorted = sortMembers([
      member("zed"),
      member("bob", { role: "admin" }),
      member("ada"),
      member("me", { you: true }),
    ])
    expect(sorted.map((m) => m.username)).toStrictEqual(["me", "bob", "ada", "zed"])
  })

  it("filters by every word, in names and groups", () => {
    const all = [
      member("ada", { displayName: "Ada Lovelace", groups: ["admins"] }),
      member("bob", { displayName: "Bob Chen" }),
    ]
    expect(filterMembers(all, "")).toHaveLength(2)
    expect(filterMembers(all, "LOVE").map((m) => m.username)).toStrictEqual(["ada"])
    expect(filterMembers(all, "admins ada").map((m) => m.username)).toStrictEqual(["ada"])
    expect(filterMembers(all, "chen ada")).toHaveLength(0)
  })

  it("asks for a backup with exactly one passkey", () => {
    expect(needsBackup(member("a"))).toBe(true)
    expect(needsBackup(member("a", { passkeys: [] }))).toBe(false)
    expect(needsBackup(member("a", { passkeys: [passkey("1"), passkey("2")] }))).toBe(false)
  })

  it("keeps your own last passkey, never someone else's", () => {
    expect(passkeyKept(member("a", { you: true }))).toBe("last")
    expect(passkeyKept(member("a", { you: true, passkeys: [passkey("1"), passkey("2")] }))).toBe(
      undefined,
    )
    expect(passkeyKept(member("a"))).toBe(undefined)
    expect(canRemoveMember(member("a", { you: true }))).toBe(false)
    expect(canRemoveMember(member("a"))).toBe(true)
  })

  it("says why links cannot be made or would not work", () => {
    expect(linkState({})).toBe("no-issuer")
    expect(linkState({ issuer: "https://id.example.com", accepted: false })).toBe("not-redeemed")
    expect(linkState({ issuer: "https://id.example.com" })).toBe("ready")
    expect(linkState({ issuer: "https://id.example.com", accepted: true })).toBe("ready")
  })

  it("checks user names as the User schema does", () => {
    for (const good of ["ada", "ada.lovelace", "a-1", "a".repeat(63)])
      expect(isUsername(good)).toBe(true)
    for (const bad of ["", "Ada", "ada_l", "-ada", "ada-", "a..b", "zeabur:ada", "a".repeat(64)])
      expect(isUsername(bad)).toBe(false)
  })

  it("tells a new member from one who gets another passkey", () => {
    const all = [member("ada")]
    expect(inviteTarget(all, " ada ")).toBe("existing")
    expect(inviteTarget(all, "cy")).toBe("new")
    expect(inviteTarget(all, "Cy")).toBe("invalid")
  })

  it("counts what a reset takes away", () => {
    const m = member("ada", {
      passkeys: [passkey("1"), passkey("2")],
      linked: [{ id: "g", provider: "GitHub", origin }],
      sessions: 3,
    })
    expect(losses(m)).toStrictEqual({ passkeys: 2, linked: 1, sessions: 3 })
  })

  it("names a member by display name, else user name", () => {
    expect(memberName(member("ada", { displayName: " Ada " }))).toBe("Ada")
    expect(memberName(member("ada", { displayName: "  " }))).toBe("ada")
    expect(memberName(member("ada"))).toBe("ada")
  })

  it("leaves out expired invitations", () => {
    const at = (expiresAt: string) => ({
      id: "i",
      name: "inv-1",
      username: "cy",
      expiresAt,
      newMember: true,
      origin,
    })
    const now = Date.parse("2026-10-22T00:00:00Z")
    expect(
      pendingInvitations([at("2026-10-21T00:00:00Z"), at("2026-10-23T00:00:00Z")], now),
    ).toHaveLength(1)
  })
})
