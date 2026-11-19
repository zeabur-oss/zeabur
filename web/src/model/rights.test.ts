import { describe, expect, test } from "bun:test"

import type { Schemas } from "@/api/client"
import { askerOf, gate, needsOf, projectPermission, rightOfResource } from "@/model/rights"

const member = askerOf({ member: true })
const kubeconfig = askerOf({ member: false })

const rights: Schemas.Rights = {
  rights: [{ right: "openTerminal", role: "developer", rule: "create pods/exec" }],
  resources: [{ resource: "statefulsets", edit: "changeServices", delete: "deleteServices" }],
}

describe("closing an action by the person's rights", () => {
  test("only a denial closes it, saying whether the person is a member", () => {
    expect(gate("deploy", "allowed", member)).toStrictEqual({ open: true })
    expect(gate("deploy", "unknown", member)).toStrictEqual({ open: true })
    expect(gate("deploy", undefined, member)).toStrictEqual({ open: true })
    expect(gate("deploy", "denied", member)).toStrictEqual({
      open: false,
      right: "deploy",
      member: true,
    })
    expect(gate("openTerminal", "denied", kubeconfig)).toMatchObject({ member: false })
  })

  test("what holds a right comes from the server's rights", () => {
    expect(needsOf(rights, "openTerminal")).toMatchObject({ role: "developer" })
    expect(needsOf(rights, "deploy")).toBeUndefined()
    expect(needsOf(undefined, "deploy")).toBeUndefined()
  })

  test("a read-only project closes its rights, and an unknown answer leaves them open", () => {
    expect(projectPermission(true)).toBe("denied")
    expect(projectPermission(false)).toBe("allowed")
    expect(projectPermission(undefined)).toBe("unknown")
  })

  test("an object's YAML is closed by the right the server names for its kind", () => {
    expect(rightOfResource(rights, "statefulsets", "delete")).toBe("deleteServices")
    expect(rightOfResource(rights, "widgets", "edit")).toBeUndefined()
    expect(rightOfResource(undefined, "statefulsets", "edit")).toBeUndefined()
  })
})
