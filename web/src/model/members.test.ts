import { describe, expect, it } from "bun:test"

import type { Problem } from "@/api/client"
import {
  accessGains,
  accessLosses,
  accessSummary,
  canChangeAccess,
  canRemoveMember,
  chosenOf,
  filterMembers,
  grantableProjects,
  grantsOf,
  inviteTarget,
  isUsername,
  lacksProjectRole,
  linkState,
  losses,
  memberName,
  membersLayout,
  needsBackup,
  passkeyKept,
  pendingInvitations,
  pickerHasFilter,
  pickerProjects,
  sortMembers,
  writeCounts,
  writtenName,
  type Member,
  type Passkey,
  type ProjectGrant,
  type ProjectRole,
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
    expect(filterMembers(all, "")).toStrictEqual(all)
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
      pendingInvitations([at("2026-10-21T00:00:00Z"), at("2026-10-23T00:00:00Z")], now).map(
        (i) => i.expiresAt,
      ),
    ).toStrictEqual(["2026-10-23T00:00:00Z"])
  })
})

describe("project roles", () => {
  const grant = (project: string, role: ProjectRole): ProjectGrant => ({ project, role })

  it("sums up an admin, no roles, unknown roles and roles per project", () => {
    expect(accessSummary(member("ada", { role: "admin" }))).toStrictEqual({ kind: "admin" })
    expect(accessSummary(member("bob"))).toStrictEqual({ kind: "unknown" })
    expect(accessSummary(member("bob", { projects: [] }))).toStrictEqual({ kind: "none" })
    expect(
      accessSummary(
        member("bob", { projects: [grant("shop", "viewer"), grant("blog", "developer")] }),
      ),
    ).toStrictEqual({
      kind: "projects",
      roles: [
        { role: "developer", named: ["blog"], count: 1 },
        { role: "viewer", named: ["shop"], count: 1 },
      ],
    })
  })

  it("names up to three projects in a role, and counts more", () => {
    const names = ["a", "b", "c", "d"]
    const few = accessSummary(
      member("cy", { projects: names.slice(0, 3).map((n) => grant(n, "viewer")) }),
    )
    expect(few).toStrictEqual({
      kind: "projects",
      roles: [{ role: "viewer", named: ["a", "b", "c"], count: 3 }],
    })
    const many = accessSummary(member("cy", { projects: names.map((n) => grant(n, "viewer")) }))
    expect(many).toStrictEqual({
      kind: "projects",
      roles: [{ role: "viewer", named: [], count: 4 }],
    })
  })

  it("names projects by their display name, in that order, else by namespace", () => {
    const titles: Record<string, string> = { "zeabur-e2e": "Shop", blog: "Another blog" }
    const summary = accessSummary(
      member("cy", {
        projects: [grant("zeabur-e2e", "viewer"), grant("blog", "viewer"), grant("api", "viewer")],
      }),
      (project) => titles[project] ?? project,
    )
    expect(summary).toStrictEqual({
      kind: "projects",
      roles: [{ role: "viewer", named: ["Another blog", "Shop", "api"], count: 3 }],
    })
  })

  it("offers every project but Kubernetes' own, and keeps one already held", () => {
    const projects = [
      { name: "shop", system: false },
      { name: "kube-system", system: true },
      { name: "blog", system: false },
    ]
    expect(grantableProjects(projects)).toStrictEqual(["blog", "shop"])
    expect(grantableProjects(projects, ["gone", "shop"])).toStrictEqual(["blog", "gone", "shop"])
  })

  it("filters the picker without hiding a chosen project", () => {
    const chosen = new Map<string, ProjectRole>([["blog", "viewer"]])
    expect(pickerProjects(["blog", "shop", "shop-api"], chosen, " Shop")).toStrictEqual([
      "blog",
      "shop",
      "shop-api",
    ])
    expect(pickerProjects(["blog", "shop"], chosen, "api")).toStrictEqual(["blog"])
    expect(pickerProjects(["a", "b", "c"], chosen, "", new Set(["c"]))).toStrictEqual([
      "c",
      "a",
      "b",
    ])
    const title = (project: string) => (project === "zeabur-e2e" ? "Shop" : project)
    expect(
      pickerProjects(["blog", "zeabur-e2e"], new Map(), "shop", new Set(), title),
    ).toStrictEqual(["zeabur-e2e"])
    expect(pickerHasFilter(7)).toBe(false)
    expect(pickerHasFilter(8)).toBe(true)
  })

  it("turns a choice into a request and a member's roles back into a choice", () => {
    const chosen = chosenOf(
      member("bob", { projects: [grant("shop", "developer"), grant("blog", "viewer")] }),
    )
    expect(grantsOf(chosen)).toStrictEqual([grant("blog", "viewer"), grant("shop", "developer")])
    expect(chosenOf(member("bob")).size).toBe(0)
  })

  it("names what a change of access gives", () => {
    const bob = member("bob", { projects: [grant("shop", "viewer"), grant("blog", "viewer")] })
    expect(
      accessGains(
        bob,
        false,
        new Map([
          ["shop", "developer"],
          ["blog", "viewer"],
          ["api", "viewer"],
        ]),
      ),
    ).toStrictEqual({ admin: false, added: [grant("api", "viewer")], raised: ["shop"] })
    expect(accessGains(undefined, false, new Map([["shop", "developer"]])).added).toStrictEqual([
      grant("shop", "developer"),
    ])
    expect(accessGains(bob, true, new Map()).admin).toBe(true)
    expect(accessGains(member("ada", { role: "admin" }), true, new Map()).admin).toBe(false)
  })

  it("names a plan's objects and counts its writes", () => {
    expect(writtenName({ kind: "Invitation", name: "inv-" })).toBe("inv-…")
    expect(writtenName({ kind: "RoleBinding", name: "zeabur-member-cy" })).toBe("zeabur-member-cy")
    expect(
      writeCounts([{ action: "delete" }, { action: "create" }, { action: "delete" }]),
    ).toStrictEqual({ create: 1, patch: 0, delete: 2 })
  })

  it("names what a change of access takes away", () => {
    const bob = member("bob", { projects: [grant("shop", "developer"), grant("blog", "viewer")] })
    expect(accessLosses(bob, false, new Map([["shop", "viewer"]]))).toStrictEqual({
      admin: false,
      removed: ["blog"],
      lowered: ["shop"],
    })
    expect(accessLosses(bob, true, new Map())).toStrictEqual({
      admin: false,
      removed: [],
      lowered: [],
    })
    expect(accessLosses(member("ada", { role: "admin" }), false, new Map()).admin).toBe(true)
  })

  it("changes access only when the member's roles are known, and never your own", () => {
    expect(canChangeAccess(member("bob"), true)).toBe(false)
    expect(canChangeAccess(member("bob"), false)).toBe(true)
    expect(canChangeAccess(member("bob", { projects: [] }), true)).toBe(true)
    expect(canChangeAccess(member("ada", { role: "admin" }), true)).toBe(true)
    expect(canChangeAccess(member("ada", { role: "admin", you: true }), true)).toBe(false)
  })

  it("tells a member without a role apart from an admin or a kubeconfig refused", () => {
    const refused = {
      type: "/problems/forbidden",
      title: "Forbidden",
      status: 403,
      detail: "",
      fix: "",
      kind: "forbidden",
      kubernetes: { code: 403, reason: "Forbidden", message: "" },
    } as Problem
    const bob = { username: "zeabur:bob", groups: [] }
    expect(lacksProjectRole(refused, bob)).toBe(true)
    expect(lacksProjectRole(refused, { username: "zeabur:ada", groups: ["zeabur:admins"] })).toBe(
      false,
    )
    expect(lacksProjectRole(refused, { username: "kubernetes-admin", groups: [] })).toBe(false)
    expect(lacksProjectRole(refused, undefined)).toBe(false)
    expect(lacksProjectRole({ ...refused, kind: "unavailable", kubernetes: undefined }, bob)).toBe(
      false,
    )
  })
})
