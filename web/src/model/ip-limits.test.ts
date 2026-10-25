import { describe, expect, test } from "bun:test"

import {
  blocksOf,
  incomingVerdict,
  isAddress,
  isUnchanged,
  limitChanges,
  outgoingVerdict,
  problemOf,
  sourceRangeOf,
  splitAddresses,
} from "./ip-limits"

describe("addresses typed by a person", () => {
  test("lines, commas and spaces separate entries, each once", () => {
    expect(splitAddresses("10.0.0.1\n10.0.1.0/24, 10.0.0.1  ::1")).toStrictEqual([
      "10.0.0.1",
      "10.0.1.0/24",
      "::1",
    ])
  })

  test("an address or a CIDR range passes; a name or a range too long does not", () => {
    for (const ok of ["203.0.113.7", "203.0.113.0/24", "2001:db8::/32", "::1"])
      expect(isAddress(ok)).toBe(true)
    for (const bad of [
      "example.com",
      "10.0.0.256",
      "10.0.0.0/33",
      "2001:db8::/129",
      "10.0.0.0/",
      "/24",
    ])
      expect(isAddress(bad)).toBe(false)
  })
})

describe("what Zeabur wrote, read back from the object's YAML", () => {
  test("a middleware's source range, as a block or a flow list, stops at the next key", () => {
    const block =
      "spec:\n  ipAllowList:\n    sourceRange:\n    - 203.0.113.0/24\n    - 198.51.100.7\n    ipStrategy:\n      depth: 1\n"
    expect(sourceRangeOf(block)).toStrictEqual(["203.0.113.0/24", "198.51.100.7"])
    expect(
      sourceRangeOf("spec:\n  ipAllowList:\n    sourceRange: [203.0.113.0/24, '198.51.100.7']\n"),
    ).toStrictEqual(["203.0.113.0/24", "198.51.100.7"])
    expect(sourceRangeOf(undefined)).toStrictEqual([])
  })

  test("a policy's ranges are its ipBlock entries", () => {
    const yaml =
      "egress:\n- to:\n  - podSelector: {}\n- to:\n  - ipBlock:\n      cidr: 203.0.113.0/24\n  - ipBlock:\n      cidr: 198.51.100.7/32\n"
    expect(blocksOf(yaml)).toStrictEqual(["203.0.113.0/24", "198.51.100.7/32"])
  })
})

describe("what a row says", () => {
  test("domains: free, limited while unread, the same everywhere, or not the same", () => {
    expect(incomingVerdict(2, [])).toStrictEqual({ is: "noLimit" })
    expect(incomingVerdict(2, [["10.0.0.0/8"], undefined])).toStrictEqual({ is: "limited" })
    expect(incomingVerdict(2, [["10.0.0.0/8"], ["10.0.0.0/8"]])).toStrictEqual({
      is: "only",
      addresses: ["10.0.0.0/8"],
    })
    expect(incomingVerdict(2, [["10.0.0.0/8"], ["10.1.0.0/16"]])).toStrictEqual({ is: "some" })
    expect(incomingVerdict(2, [["10.0.0.0/8"]])).toStrictEqual({ is: "some" })
  })

  test("pods: another policy's limit is not called free", () => {
    expect(outgoingVerdict(false, false, undefined)).toStrictEqual({ is: "noLimit" })
    expect(outgoingVerdict(false, true, undefined)).toStrictEqual({ is: "rules" })
    expect(outgoingVerdict(true, true, ["10.0.0.0/8"])).toStrictEqual({
      is: "only",
      addresses: ["10.0.0.0/8"],
    })
  })
})

describe("the form of a limit", () => {
  const was = { on: true, same: true, cidrs: ["10.0.0.0/8"] }

  test("domains need an address; pods may have none, which leaves the project and DNS", () => {
    expect(problemOf("in", true, [])).toBe("none")
    expect(problemOf("out", true, [])).toBeUndefined()
    expect(problemOf("out", true, ["example.com"])).toBe("bad")
    expect(problemOf("in", false, [])).toBeUndefined()
  })

  test("the same addresses, or off when off, change nothing", () => {
    expect(isUnchanged(was, true, ["10.0.0.0/8"])).toBe(true)
    expect(isUnchanged(was, true, ["10.1.0.0/16"])).toBe(false)
    expect(isUnchanged({ ...was, same: false }, true, ["10.0.0.0/8"])).toBe(false)
    expect(isUnchanged({ on: false, same: false, cidrs: [] }, false, [])).toBe(true)
    expect(isUnchanged(was, false, [])).toBe(false)
  })

  test("limiting asks of every Ingress; stopping asks only of the limited ones", () => {
    const api = { kind: "Deployment", name: "api" } as const
    const routes = [
      { name: "a", limited: true },
      { name: "b", limited: false },
    ]
    expect(limitChanges("in", api, routes, true, ["10.0.0.0/8"]).map((c) => c.op)).toStrictEqual([
      "ipAllow",
      "ipAllow",
    ])
    expect(limitChanges("in", api, routes, false, ["10.0.0.0/8"])).toStrictEqual([
      { op: "ipAllow", route: { kind: "Ingress", name: "a" }, enabled: false, cidrs: [] },
    ])
    expect(limitChanges("out", api, routes, false, ["10.0.0.0/8"])).toStrictEqual([
      { op: "egressLimit", workload: api, enabled: false, cidrs: [] },
    ])
  })
})
