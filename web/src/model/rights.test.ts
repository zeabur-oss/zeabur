import { describe, expect, test } from "bun:test"

import {
  askerOf,
  gate,
  mayActAsClusterAdmin,
  neededRole,
  rightOfResource,
  ruleOf,
  unknownRights,
} from "@/model/rights"

const member = askerOf({ username: "zeabur:bob", groups: ["zeabur:members"] }, "available")
const admin = askerOf({ username: "zeabur:alice", groups: ["zeabur:admins"] }, "available")
const kubeconfig = askerOf({ username: "carol", groups: [] }, "unavailable")

describe("closing an action by the person's rights", () => {
  test("only a denial closes it, naming the role a member needs", () => {
    expect(gate("deploy", "allowed", member)).toStrictEqual({ open: true })
    expect(gate("deploy", "unknown", member)).toStrictEqual({ open: true })
    expect(gate("deploy", undefined, member)).toStrictEqual({ open: true })
    expect(gate("deploy", "denied", member)).toMatchObject({
      open: false,
      role: "developer",
      member: true,
    })
    expect(gate("changeMembers", "denied", member)).toMatchObject({ role: "admin" })
  })

  test("an admin who may take admin rights keeps it open; the plan offers them", () => {
    expect(gate("installComponents", "denied", admin)).toStrictEqual({ open: true })
    const elevated = askerOf({ username: "zeabur:alice", groups: ["zeabur:admins"] }, "active")
    expect(gate("installComponents", "denied", elevated).open).toBe(false)
    // Admin rights offered to someone outside the admins group would be refused by Zeabur Identity.
    expect(gate("installComponents", "denied", member).open).toBe(false)
  })

  test("node and cluster terminals are offered to cluster administrators and admins who may elevate", () => {
    const elevated = askerOf({ username: "zeabur:alice", groups: ["zeabur:admins"] }, "active")
    expect(mayActAsClusterAdmin(true, kubeconfig)).toBe(true)
    expect(mayActAsClusterAdmin(false, admin)).toBe(true)
    expect(mayActAsClusterAdmin(false, elevated)).toBe(false)
    expect(mayActAsClusterAdmin(false, member)).toBe(false)
    expect(mayActAsClusterAdmin(undefined, kubeconfig)).toBe(false)
  })

  test("someone signed in with a kubeconfig is told the rule, not a role", () => {
    expect(gate("openTerminal", "denied", kubeconfig)).toMatchObject({
      open: false,
      member: false,
      rule: "create pods/exec",
    })
  })

  test("members, volume restores, components and nodes need an admin; everything else in a project a developer", () => {
    const admin = new Set(["changeMembers", "restoreVolumes"])
    for (const right of Object.keys(unknownRights()) as (keyof ReturnType<typeof unknownRights>)[])
      expect(neededRole(right)).toBe(admin.has(right) ? "admin" : "developer")
    expect(neededRole("installComponents")).toBe("admin")
    expect(neededRole("changeNodes")).toBe("admin")
    expect(ruleOf("readSecrets")).toBe("get secrets")
  })

  test("an object's YAML is closed by the right for its kind, and any other kind is left open", () => {
    expect(rightOfResource("deployments", "edit")).toBe("changeServices")
    expect(rightOfResource("statefulsets", "delete")).toBe("deleteServices")
    expect(rightOfResource("secrets", "delete")).toBe("changeSecrets")
    expect(rightOfResource("httproutes", "edit")).toBe("changeDomains")
    expect(rightOfResource("widgets", "edit")).toBeUndefined()
  })
})
