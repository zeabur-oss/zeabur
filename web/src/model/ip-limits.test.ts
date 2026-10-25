import { describe, expect, test } from "bun:test"

import { blocksOf, isAddress, sourceRangeOf, splitAddresses } from "./ip-limits"

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
