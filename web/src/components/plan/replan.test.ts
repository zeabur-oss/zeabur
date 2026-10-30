import { describe, expect, test } from "bun:test"

import type { Problem, Schemas } from "@/api/client"
import { changedSince, replansOn } from "@/components/plan/replan"

const object = (kind: string, name: string, type: "patch" | "delete" = "delete") =>
  ({
    target: { apiVersion: "v1", kind, plural: `${kind.toLowerCase()}s`, name },
    changes: [],
    owners: [],
    write: { type },
  }) as unknown as Schemas.PlannedObject

const problem = (over: Partial<Problem>): Problem =>
  ({ kind: "conflict", title: "", status: 409, detail: "", ...over }) as Problem

describe("an apply refused because the project moved on", () => {
  test("plans again when nothing was written", () => {
    expect(replansOn(problem({}))).toBe(true)
  })

  test("does not plan again after some objects were written", () => {
    const applied = [{ apiVersion: "v1", kind: "Service", plural: "services", name: "web" }]
    expect(replansOn(problem({ applied } as Partial<Problem>))).toBe(false)
  })

  test("does not plan again when a tool would undo the change", () => {
    expect(replansOn(problem({ managedBy: "Helm" }))).toBe(false)
  })

  test("does not plan again for other refusals", () => {
    expect(replansOn(problem({ kind: "plan-expired", status: 410 }))).toBe(false)
    expect(replansOn(problem({ kind: "forbidden", status: 403 }))).toBe(false)
  })

  test("names what the new plan adds and drops", () => {
    // A worker that shared web-env was deleted: the Secret now goes with web.
    const reviewed = {
      objects: [object("Deployment", "web"), object("Service", "web"), object("ConfigMap", "old")],
    }
    const now = {
      objects: [object("Deployment", "web"), object("Service", "web"), object("Secret", "web-env")],
    }
    expect(changedSince(reviewed, now)).toEqual({
      added: ["Secret web-env"],
      dropped: ["ConfigMap old"],
    })
  })

  test("names nothing when only a version moved", () => {
    const reviewed = { objects: [object("Deployment", "web", "patch")] }
    expect(changedSince(reviewed, reviewed)).toEqual({ added: [], dropped: [] })
  })
})
