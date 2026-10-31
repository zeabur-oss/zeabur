import { describe, expect, setSystemTime, test } from "bun:test"

import {
  CHECK_EVERY,
  RELEASES_URL,
  compareVersions,
  findNewer,
  publishedReleases,
  parseVersion,
  releases,
  updateCheckQuery,
  type Release,
} from "@/model/update-check"

const v = (text: string) => parseVersion(text)!

const github = (tag: string, body = "", extra: Record<string, unknown> = {}) => ({
  tag_name: tag,
  html_url: `https://github.com/zeabur-oss/zeabur/releases/tag/${tag}`,
  draft: false,
  prerelease: tag.includes("-"),
  body,
  ...extra,
})

function memory(initial: Record<string, string> = {}) {
  const values = new Map(Object.entries(initial))
  return {
    values,
    load: (key: string) => values.get(key) ?? null,
    save: (key: string, value: string) => void values.set(key, value),
  }
}

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
    const list = publishedReleases([
      github("v0.3.0-beta.1"),
      github("v0.4.0", "", { draft: true }),
      github("v0.1.2"),
      github("v0.2.0"),
    ])
    expect(findNewer(list, "0.1.1")?.latest.version).toBe("0.2.0")
    expect(findNewer(list, "0.2.0")).toBeUndefined()
    expect(findNewer(publishedReleases([github("v0.1.1-beta.1")]), "0.1.0")).toBeUndefined()
    expect(findNewer([], "0.1.0")).toBeUndefined()
  })

  test("someone on a pre-release hears of newer pre-releases", () => {
    const list = publishedReleases([
      github("v0.2.0-rc.2", "### Security\n- In a pre-release"),
      github("v0.1.1"),
    ])
    const found = findNewer(list, "0.2.0-rc.1")
    expect(found?.latest.version).toBe("0.2.0-rc.2")
    expect(found?.security).toEqual([])
    expect(findNewer(list, "0.1.1")).toBeUndefined()
    const released = publishedReleases([github("v0.2.0"), github("v0.2.0-rc.2")])
    expect(findNewer(released, "0.2.0-rc.1")?.latest.version).toBe("0.2.0")
  })

  test("a pre-release is behind its normal release", () => {
    expect(findNewer(publishedReleases([github("v0.1.1")]), "0.1.1-beta.2")?.latest.version).toBe(
      "0.1.1",
    )
  })

  test("names the newer releases with a Security section, not older ones", () => {
    const list = publishedReleases([
      github("v0.3.0", "### Added\n- Things"),
      github("v0.2.0", "## 0.2.0\r\n\r\n### Security\r\n- Fixed a hole"),
      github("v0.1.0", "### Security\n- Older"),
      github("v0.1.5", "Security fixes are below.\n#### Security"),
    ])
    expect(findNewer(list, "0.1.0")?.security.map((r) => r.version)).toEqual(["0.2.0"])
    expect(findNewer(list, "0.2.0")?.security).toEqual([])
  })
})

describe("checking once a day", () => {
  const answer = [github("v0.2.0", "### Security\n- x")]
  const ok = () => Promise.resolve(new Response(JSON.stringify(answer)))

  test("asks GitHub without a referrer or credentials, then keeps the answer for a day", async () => {
    const storage = memory()
    const calls: [string, RequestInit][] = []
    const fetcher = (url: string, init: RequestInit) => {
      calls.push([url, init])
      return ok()
    }
    const first = await releases(fetcher, storage, 1_000)
    expect(first.map((r: Release) => [r.version, r.security])).toEqual([["0.2.0", true]])
    expect(calls).toHaveLength(1)
    expect(calls[0]![0]).toBe(RELEASES_URL)
    expect(calls[0]![1].referrerPolicy).toBe("no-referrer")
    expect(calls[0]![1].credentials).toBe("omit")

    await releases(fetcher, storage, 1_000 + CHECK_EVERY - 1)
    expect(calls).toHaveLength(1)
    await releases(fetcher, storage, 1_000 + CHECK_EVERY)
    expect(calls).toHaveLength(2)
  })

  test("a failed request keeps the last answer and does not ask again that day", async () => {
    const storage = memory()
    await releases(ok, storage, 0)
    let asked = 0
    const down = () => {
      asked++
      return Promise.reject(new TypeError("Failed to fetch"))
    }
    const kept = await releases(down, storage, CHECK_EVERY)
    expect(kept.map((r) => r.version)).toEqual(["0.2.0"])
    await releases(down, storage, CHECK_EVERY + 1)
    expect(asked).toBe(1)
  })

  test("an unreadable cache is a fresh start", async () => {
    const storage = memory({ "zeabur.updateCheck": "{not json" })
    const list = await releases(ok, storage, 0)
    expect(list.map((r) => r.version)).toEqual(["0.2.0"])
  })
})

describe("a page left open", () => {
  test("polls daily, and each poll asks GitHub only once the day's cache has expired", async () => {
    const storage = memory()
    let requests = 0
    const fetcher = async () => {
      requests++
      return { ok: true, json: async () => [github("v1.0.0")] } as Response
    }
    const options = updateCheckQuery(fetcher, storage, true)
    expect(options.refetchInterval).toBe(CHECK_EVERY)
    const poll = async () => await (options.queryFn as () => Promise<Release[]>)()
    try {
      await poll()
      await poll()
      expect(requests).toBe(1)
      setSystemTime(new Date(Date.now() + CHECK_EVERY + 1))
      await poll()
      expect(requests).toBe(2)
    } finally {
      setSystemTime()
    }
  })
})
