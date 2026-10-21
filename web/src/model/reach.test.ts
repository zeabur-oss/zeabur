import { describe, expect, test } from "bun:test"

import type { Schemas } from "@/api/client"
import { filterPeers, FEW_PEERS, leadOf, namesShown, reachSummary } from "@/model/reach"

type Reach = Schemas["NetworkPolicyService"]
type Access = Schemas["NetworkPolicyAccess"]

const reach = (accesses: Access[], selectedBy = ["default-deny"]): Reach => ({
  service: "p/deployment/me",
  selectedBy,
  ingressIsolated: selectedBy.length > 0,
  egressIsolated: false,
  fromIngressController: "allowed",
  fromOtherNamespaces: "blocked",
  toDns: "allowed",
  toOutside: "allowed",
  fromServices: accesses.map((access, i) => ({
    service: `p/deployment/s${String(i).padStart(2, "0")}`,
    access,
  })),
})
const nameOf = (id: string) => id.split("/").pop() ?? id

describe("reachSummary", () => {
  test("no rule: open, nothing grouped", () => {
    const s = reachSummary(reach([], []), nameOf)
    expect(s.open).toBe(true)
    expect(s.groups).toEqual([])
    expect(s.few).toBe(true)
  })

  test("one peer", () => {
    const s = reachSummary(reach(["blocked"]), nameOf)
    expect(s.open).toBe(false)
    expect(s.groups.map((g) => [g.access, g.peers.length])).toEqual([["blocked", 1]])
  })

  test("few peers: exceptions first", () => {
    const s = reachSummary(reach(["allowed", "blocked", "limited", "allowed"]), nameOf)
    expect(s.few).toBe(true)
    expect(s.groups.map((g) => g.access)).toEqual(["blocked", "limited", "allowed"])
  })

  test("many peers fold: five blocked names then a count, allowed only counted", () => {
    const accesses: Access[] = Array.from({ length: 47 }, (_, i) =>
      i < 32 ? "allowed" : "blocked",
    )
    const s = reachSummary(reach(accesses), nameOf)
    expect(s.few).toBe(false)
    const [blocked, allowed] = s.groups
    expect(namesShown(blocked!)).toEqual({
      names: ["s32", "s33", "s34", "s35", "s36"],
      more: 10,
    })
    expect(namesShown(allowed!)).toEqual({ names: [], more: 32 })
  })

  test("the threshold is inclusive", () => {
    expect(reachSummary(reach(Array(FEW_PEERS).fill("allowed")), nameOf).few).toBe(true)
    expect(reachSummary(reach(Array(FEW_PEERS + 1).fill("allowed")), nameOf).few).toBe(false)
  })
})

test("leadOf counts each outcome", () => {
  expect(leadOf(reach(["allowed", "blocked", "blocked"]))).toEqual({
    allowed: 1,
    limited: 0,
    blocked: 2,
    domains: "allowed",
    otherProjects: "blocked",
  })
})

test("filterPeers matches part of a name, ignoring case and spaces", () => {
  const peers = [
    { id: "a", name: "Billing-Sync" },
    { id: "b", name: "db" },
  ]
  expect(filterPeers(peers, " sync ")).toEqual([peers[0]!])
  expect(filterPeers(peers, "")).toEqual(peers)
})
