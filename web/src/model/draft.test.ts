import { describe, expect, test } from "bun:test"

import type { Schemas } from "@/api/client"
import {
  draftable,
  dropChanges,
  editCount,
  mergeChanges,
  partOf,
  stagedDomains,
  stagedSettings,
} from "@/model/draft"

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

  test("an edit taken back out leaves the rest staged", () => {
    const add: Change = { op: "addDomain", workload, hostname: "a.example.com" }
    const staged = mergeChanges([env(["A", "1"])], [add])
    expect(dropChanges(staged, [add])).toEqual([env(["A", "1"])])
  })

  test("a workload's staged domains: those added, and edits of its routes' hosts", () => {
    const other = { kind: "Deployment", name: "web" } as Schemas["WorkloadRef"]
    const route = { kind: "Ingress", name: "api" } as Schemas["RouteRef"]
    const { added, edits } = stagedDomains(
      [
        { op: "addDomain", workload, hostname: "a.example.com" },
        { op: "addDomain", workload: other, hostname: "b.example.com" },
        { op: "removeDomain", route, hostname: "old.example.com" },
        { op: "renameDomain", route: { ...route, name: "web" }, from: "c.example.com", to: "d" },
      ],
      workload,
      ["api"],
    )
    expect(added.map((c) => c.hostname)).toEqual(["a.example.com"])
    expect([...edits.keys()]).toEqual(["api/old.example.com"])
  })

  test("a domain's drawers wait in the draft, and a later setting replaces the earlier one", () => {
    const route = { kind: "Ingress", name: "api" } as Schemas["RouteRef"]
    const redirect = (to: string): Change => ({
      op: "redirect",
      route,
      workload,
      hostname: "a.example.com",
      to,
    })
    const password: Change = { op: "basicAuth", route, user: { username: "u", password: "p" } }
    const pages: Change = { op: "maintenance", route, hostname: "b.example.com", enabled: true }
    expect(draftable([redirect("x"), password, pages])).toBe(true)
    const staged = mergeChanges([redirect("x"), password, pages], [redirect("y")])
    expect(staged).toEqual([redirect("y"), password, pages])
    expect(stagedSettings(staged, "api", "a.example.com")).toEqual([redirect("y"), password])
    expect(stagedSettings(staged, "web", "a.example.com")).toEqual([])
  })

  test("pages typed in two drawers become one change", () => {
    const merged = mergeChanges(
      [{ op: "errorPageHtml", maintenance: "<p>a</p>" }],
      [{ op: "errorPageHtml", badGateway: "<p>b</p>" }],
    )
    expect(merged).toEqual([
      { op: "errorPageHtml", maintenance: "<p>a</p>", badGateway: "<p>b</p>" },
    ])
  })
})
