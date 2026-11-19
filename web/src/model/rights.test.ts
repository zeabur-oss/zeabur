import { describe, expect, test } from "bun:test"

import {
  askerOf,
  gate,
  neededRole,
  projectPermission,
  rightOfResource,
  ruleOf,
} from "@/model/rights"

const member = askerOf({ username: "zeabur:bob" })
const admin = askerOf({ username: "zeabur:alice" })
const kubeconfig = askerOf({ username: "carol" })

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

  test("a denial closes it for an admin account too", () => {
    expect(gate("changeNodes", "denied", admin)).toMatchObject({ open: false, role: "admin" })
  })

  test("someone signed in with a kubeconfig is told the rule, not a role", () => {
    expect(gate("openTerminal", "denied", kubeconfig)).toMatchObject({
      open: false,
      member: false,
      rule: "create pods/exec",
    })
  })

  test("members and nodes need an admin; everything else in a project a developer", () => {
    expect(neededRole("changeMembers")).toBe("admin")
    expect(neededRole("deploy")).toBe("developer")
    expect(neededRole("changeNodes")).toBe("admin")
    expect(ruleOf("readSecrets")).toBe("get secrets")
  })

  test("a read-only project closes its rights, and an unknown answer leaves them open", () => {
    expect(projectPermission(true)).toBe("denied")
    expect(projectPermission(false)).toBe("allowed")
    expect(projectPermission(undefined)).toBe("unknown")
  })

  test("an object's YAML is closed by the right for its kind, and any other kind is left open", () => {
    expect(rightOfResource("deployments", "edit")).toBe("changeServices")
    expect(rightOfResource("statefulsets", "delete")).toBe("deleteServices")
    expect(rightOfResource("secrets", "delete")).toBe("changeSecrets")
    expect(rightOfResource("httproutes", "edit")).toBe("changeDomains")
    expect(rightOfResource("widgets", "edit")).toBeUndefined()
  })
})
