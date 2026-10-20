import { describe, expect, test } from "bun:test"

import type { Schemas } from "@/api/client"
import { draftable, editCount, mergeChanges, partOf } from "@/model/draft"

type Change = Schemas["Change"]

const workload = { kind: "Deployment", name: "api" } as Schemas["WorkloadRef"]
const env = (...pairs: [string, string][]): Change => ({
  op: "env",
  workload,
  changes: pairs.map(([name, value]) => ({ name, op: "set", value })),
})

describe("draft", () => {
  test("each edit belongs to one part of Configure", () => {
    expect(partOf(env(["A", "1"]))).toBe("variables")
    expect(partOf({ op: "scale", workload, replicas: 2 } as Change)).toBe("compute")
    expect(partOf({ op: "addDomain", workload, hostname: "a.example.com" })).toBe("networking")
  })

  test("only changes with a part wait in the draft", () => {
    expect(draftable([env(["A", "1"])])).toBe(true)
    expect(draftable([])).toBe(false)
    expect(draftable([{ op: "restart", workload } as Change])).toBe(false)
  })

  test("a later edit of the same variable replaces the earlier one", () => {
    const merged = mergeChanges([env(["A", "1"], ["B", "2"])], [env(["A", "3"])])
    expect(merged).toHaveLength(1)
    const only = merged[0]
    expect(only?.op === "env" ? only.changes.map((c) => `${c.name}=${c.value}`) : []).toEqual([
      "B=2",
      "A=3",
    ])
    expect(editCount(merged)).toBe(2)
  })

  test("a later scale replaces the earlier one and counts once", () => {
    const merged = mergeChanges(
      [{ op: "scale", workload, replicas: 2 } as Change],
      [{ op: "scale", workload, replicas: 4 } as Change, env(["A", "1"])],
    )
    expect(merged).toHaveLength(2)
    expect(editCount(merged)).toBe(2)
  })
})
