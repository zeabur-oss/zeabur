import { describe, expect, test } from "bun:test"

import {
  compareVersions,
  findNewer,
  newerFor,
  parseVersion,
  type Release,
} from "@/model/update-check"

const v = (text: string) => parseVersion(text)!

const release = (version: string, security = false): Release => ({
  version,
  url: `https://github.com/zeabur-oss/zeabur/releases/tag/v${version}`,
  prerelease: version.includes("-"),
  security,
})

describe("versions", () => {
  test("follow SemVer precedence, pre-releases before their normal version", () => {
    const ordered = [
      "1.0.0-alpha",
      "1.0.0-alpha.1",
      "1.0.0-alpha.beta",
      "1.0.0-beta",
      "1.0.0-beta.2",
      "1.0.0-beta.11",
      "1.0.0-rc.1",
      "1.0.0",
      "1.0.1",
      "1.10.0",
      "2.0.0",
    ]
    for (let i = 1; i < ordered.length; i++)
      expect(compareVersions(v(ordered[i - 1]!), v(ordered[i]!))).toBeLessThan(0)
    expect(compareVersions(v("v1.2.3"), v("1.2.3+build.5"))).toBe(0)
    for (const bad of ["1.2", "1.2.3.4", "x.1.2", "1.2.3-", "1.2.3-a..b", ""])
      expect(parseVersion(bad)).toBeUndefined()
  })
})

describe("finding a newer release", () => {
  test("only normal releases count, and the newest of them is the latest", () => {
    const list = [release("0.3.0-beta.1"), release("0.1.2"), release("0.2.0")]
    expect(findNewer(list, "0.1.1")?.latest.version).toBe("0.2.0")
    expect(findNewer(list, "0.2.0")).toBeUndefined()
    expect(findNewer([release("0.1.1-beta.1")], "0.1.0")).toBeUndefined()
    expect(findNewer([], "0.1.0")).toBeUndefined()
  })

  test("someone on a pre-release hears of newer pre-releases", () => {
    const list = [release("0.2.0-rc.2", true), release("0.1.1")]
    const found = findNewer(list, "0.2.0-rc.1")
    expect(found?.latest.version).toBe("0.2.0-rc.2")
    expect(found?.security).toEqual([])
    expect(findNewer(list, "0.1.1")).toBeUndefined()
    const released = [release("0.2.0"), release("0.2.0-rc.2")]
    expect(findNewer(released, "0.2.0-rc.1")?.latest.version).toBe("0.2.0")
  })

  test("a tag that is no version is passed over", () => {
    expect(findNewer([release("nightly"), release("0.2.0")], "0.1.0")?.latest.version).toBe("0.2.0")
  })

  test("a pre-release is behind its normal release", () => {
    expect(findNewer([release("0.1.1")], "0.1.1-beta.2")?.latest.version).toBe("0.1.1")
  })

  test("names the newer releases with a Security section, not older ones", () => {
    const list = [
      release("0.3.0"),
      release("0.2.0", true),
      release("0.1.0", true),
      release("0.1.5"),
    ]
    expect(findNewer(list, "0.1.0")?.security.map((r) => r.version)).toEqual(["0.2.0"])
    expect(findNewer(list, "0.2.0")?.security).toEqual([])
  })
})

describe("a cluster that opts out of update checks", () => {
  test("hears of nothing from releases cached before it opted out", () => {
    const list = [release("0.2.0")]
    expect(newerFor({ version: "0.1.0", updateCheck: true }, list)?.latest.version).toBe("0.2.0")
    expect(newerFor({ version: "0.1.0", updateCheck: false }, list)).toBeUndefined()
  })
})
