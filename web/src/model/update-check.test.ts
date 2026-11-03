import { describe, expect, test } from "bun:test"
import { QueryClient, QueryObserver } from "@tanstack/react-query"

import {
  CHECK_EVERY,
  RELEASES_URL,
  compareVersions,
  findNewer,
  newerFor,
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

describe("a cluster that opts out of update checks", () => {
  test("hears of nothing from releases cached before it opted out", () => {
    const list = publishedReleases([github("v0.2.0")])
    expect(newerFor({ version: "0.1.0", updateCheck: true }, list)?.latest.version).toBe("0.2.0")
    expect(newerFor({ version: "0.1.0", updateCheck: false }, list)).toBeUndefined()
  })
})

describe("checking each time a page opens", () => {
  const answer = [github("v0.2.0", "### Security\n- x")]
  const ok = () => Promise.resolve(new Response(JSON.stringify(answer)))

  test("asks GitHub without a referrer or credentials, and keeps the answer", async () => {
    const storage = memory()
    const calls: [string, RequestInit][] = []
    const fetcher = (url: string, init: RequestInit) => {
      calls.push([url, init])
      return ok()
    }
    const first = await releases(fetcher, storage, 1_000)
    expect(first.map((r: Release) => [r.version, r.security])).toEqual([["0.2.0", true]])
    expect(calls[0]![0]).toBe(RELEASES_URL)
    expect(calls[0]![1].referrerPolicy).toBe("no-referrer")
    expect(calls[0]![1].credentials).toBe("omit")
    expect(JSON.parse(storage.values.get("zeabur.updateCheck")!).releases).toEqual(first)
  })

  test("a failed or refused request leaves the last answer", async () => {
    const storage = memory()
    await releases(ok, storage, 0)
    const kept = storage.values.get("zeabur.updateCheck")
    const down = () => Promise.reject(new TypeError("Failed to fetch"))
    const limited = () => Promise.resolve(new Response("{}", { status: 403 }))
    for (const fetcher of [down, limited]) {
      await expect(releases(fetcher, storage, 1)).rejects.toThrow()
      expect(storage.values.get("zeabur.updateCheck")).toBe(kept)
    }
  })
})

describe("GitHub's hourly limit", () => {
  test("once used up, no request until GitHub's reset, and the last answer stands", async () => {
    const storage = memory()
    await releases(async () => new Response(JSON.stringify([github("v0.2.0")])), storage, 0)
    const reset = 3_600
    let requests = 0
    const limited = async () => {
      requests++
      return new Response("{}", {
        status: 403,
        headers: { "x-ratelimit-remaining": "0", "x-ratelimit-reset": String(reset) },
      })
    }
    await expect(releases(limited, storage, 1_000)).rejects.toThrow()
    const kept = await releases(limited, storage, reset * 1000 - 1)
    expect(kept.map((r) => r.version)).toEqual(["0.2.0"])
    expect(requests).toBe(1)
    await expect(releases(limited, storage, reset * 1000)).rejects.toThrow()
    expect(requests).toBe(2)
  })
})

describe("a page", () => {
  const cachedAnswer = (tag: string) =>
    JSON.stringify({ checkedAt: Date.now(), releases: publishedReleases([github(tag)]) })

  function observe(
    fetcher: (url: string, init: RequestInit) => Promise<Response>,
    storage: ReturnType<typeof memory>,
  ) {
    const client = new QueryClient()
    const observer = new QueryObserver(client, updateCheckQuery(fetcher, storage, true))
    const seen: (string | undefined)[] = []
    const stop = observer.subscribe((result) => seen.push(result.data?.[0]?.version))
    return { observer, seen, stop, client }
  }

  test("shows the last answer at once and asks GitHub again in the background", async () => {
    const storage = memory({ "zeabur.updateCheck": cachedAnswer("v0.2.0") })
    let requests = 0
    const fetcher = async () => {
      requests++
      return new Response(JSON.stringify([github("v0.3.0")]))
    }
    const { observer, seen, stop, client } = observe(fetcher, storage)
    expect(observer.getCurrentResult().data?.[0]?.version).toBe("0.2.0")
    await client.getQueryCache().find({ queryKey: ["update-check"] })?.promise
    expect(seen.at(-1)).toBe("0.3.0")
    expect(requests).toBe(1)
    stop()
  })

  test("keeps showing the last answer when GitHub cannot be reached", async () => {
    const storage = memory({ "zeabur.updateCheck": cachedAnswer("v0.2.0") })
    const { observer, stop, client } = observe(
      () => Promise.reject(new TypeError("Failed to fetch")),
      storage,
    )
    await client
      .getQueryCache()
      .find({ queryKey: ["update-check"] })
      ?.promise?.catch(() => {})
    expect(observer.getCurrentResult().data?.[0]?.version).toBe("0.2.0")
    stop()
  })

  test("an unreadable cache shows nothing until GitHub answers", () => {
    const storage = memory({ "zeabur.updateCheck": "{not json" })
    const { observer, stop } = observe(() => new Promise(() => {}), storage)
    expect(observer.getCurrentResult().data).toBeUndefined()
    stop()
  })

  test("left open, asks again every hour", () => {
    expect(updateCheckQuery(fetch, memory(), true).refetchInterval).toBe(CHECK_EVERY)
  })
})
