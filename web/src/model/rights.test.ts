import { describe, expect, test } from "bun:test"

import { askerOf, gate, neededRole, rowGate, ruleOf, unknownRights } from "@/model/rights"

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
    const admin = new Set(["changeMembers"])
    for (const right of Object.keys(unknownRights()) as (keyof ReturnType<typeof unknownRights>)[])
      expect(neededRole(right)).toBe(admin.has(right) ? "admin" : "developer")
    expect(neededRole("changeNodes")).toBe("admin")
    expect(ruleOf("readSecrets")).toBe("get secrets")
  })

  test("an object is closed by what the rules say about its name, naming the rule", () => {
    expect(rowGate("patch", "deployments.apps", "allowed", kubeconfig)).toStrictEqual({
      open: true,
    })
    expect(rowGate("patch", "deployments.apps", "unknown", kubeconfig)).toStrictEqual({
      open: true,
    })
    expect(rowGate("delete", "secrets", "denied", kubeconfig)).toMatchObject({
      open: false,
      rule: "delete secrets",
      member: false,
    })
  })
})
