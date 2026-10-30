import { describe, expect, test } from "bun:test"

import type { Schemas } from "@/api/client"
import {
  byRules,
  filterPeers,
  FEW_PEERS,
  leadOf,
  namesShown,
  reachSummary,
  sharedRules,
  showsRules,
} from "@/model/reach"

type Reach = Schemas.NetworkPolicyService
type Access = Schemas.NetworkPolicyAccess

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
    via: access === "blocked" ? ["default-deny"] : [`allow-s${i}`],
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

describe("rules per peer", () => {
  test("said only when the project has more than one", () => {
    expect(showsRules([])).toBe(false)
    expect(showsRules(["default-deny"])).toBe(false)
    expect(showsRules(["default-deny", "allow-web"])).toBe(true)
  })

  test("each peer keeps the rules that decide it", () => {
    const [blocked, allowed] = reachSummary(reach(["allowed", "blocked"]), nameOf).groups
    expect(blocked!.peers).toEqual([{ id: "p/deployment/s01", name: "s01", via: ["default-deny"] }])
    expect(allowed!.peers[0]!.via).toEqual(["allow-s0"])
  })

  test("peers decided by the same rules are said together, in name order", () => {
    const peer = (name: string, via: string[]) => ({ id: name, name, via })
    expect(
      byRules([
        peer("cron", ["allow-web", "deny-api"]),
        peer("db", ["allow-db"]),
        peer("worker", ["allow-web", "deny-api"]),
      ]).map((part) => [part.via, part.peers.map((p) => p.name)]),
    ).toEqual([
      [
        ["allow-web", "deny-api"],
        ["cron", "worker"],
      ],
      [["allow-db"], ["db"]],
    ])
  })

  test("rules shared by every peer are said once, else on each", () => {
    const peer = (name: string, via: string[]) => ({ id: name, name, via })
    expect(sharedRules([peer("a", ["deny-all"]), peer("b", ["deny-all"])])).toEqual(["deny-all"])
    expect(sharedRules([peer("a", ["deny-all"]), peer("b", ["allow-b"])])).toBeUndefined()
  })
})

test("filterPeers matches part of a name, ignoring case and spaces", () => {
  const peers = [
    { id: "a", name: "Billing-Sync", via: [] },
    { id: "b", name: "db", via: [] },
  ]
  expect(filterPeers(peers, " sync ")).toEqual([peers[0]!])
  expect(filterPeers(peers, "")).toEqual(peers)
})
