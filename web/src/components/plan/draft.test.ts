import { describe, expect, test } from "bun:test"

import type { Schemas } from "@/api/client"
import { covers } from "@/components/plan/draft"

const api = { kind: "Deployment", name: "api" } as Schemas["WorkloadRef"]
const web = { kind: "Deployment", name: "web" } as Schemas["WorkloadRef"]

describe("held edits", () => {
  test("stand in for staged changes of their ops and workload", () => {
    const scope = { ops: ["scale", "resources"], workload: api } as const
    const covered = (change: Schemas["Change"]) => covers({ ...scope, ops: [...scope.ops] }, change)
    expect(covered({ op: "scale", workload: api, replicas: 2 })).toBe(true)
    expect(covered({ op: "scale", workload: web, replicas: 2 })).toBe(false)
    expect(covered({ op: "env", workload: api, changes: [] })).toBe(false)
  })

  test("a probe form covers only its own probe", () => {
    const scope = { ops: ["probe" as const], workload: api, probe: "liveness" as const }
    const probe = (kind: Schemas["ProbeKind"]): Schemas["Change"] => ({
      op: "probe",
      workload: api,
      container: "api",
      probe: kind,
      set: null,
    })
    expect(covers(scope, probe("liveness"))).toBe(true)
    expect(covers(scope, probe("readiness"))).toBe(false)
  })

  test("a claim's growth has no workload and is covered by its op", () => {
    const scope = { ops: ["expandVolume" as const], workload: api }
    expect(covers(scope, { op: "expandVolume", claim: "web-data", size: "2Gi" })).toBe(true)
  })
})
