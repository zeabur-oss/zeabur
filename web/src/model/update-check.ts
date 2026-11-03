/** The update check: each time a page opens, this browser shows the releases it read last and
 * asks GitHub again in the background, then compares the newest normal one with the running
 * version. Zeabur's server makes no request. */

import { queryOptions } from "@tanstack/react-query"

export const RELEASES_URL = "https://api.github.com/repos/zeabur-oss/zeabur/releases"
/** How often a page left open asks again. */
export const CHECK_EVERY = 60 * 60_000
const CHECK_TIMEOUT = 10_000
const CACHE_KEY = "zeabur.updateCheck"

/** A published release. */
export interface Release {
  version: string
  url: string
  prerelease: boolean
  /** Its notes have a `### Security` section: it fixes vulnerabilities. */
  security: boolean
}

interface Cached {
  checkedAt: number
  releases: Release[]
  /** GitHub refused for the rest of the hour: no request until then. */
  limitedUntil?: number
}

export interface Newer {
  latest: Release
  /** Newer releases that fix vulnerabilities, newest first. */
  security: Release[]
}

interface Version {
  core: [number, number, number]
  pre: string[]
}

/** `1.2.3`, `v1.2.3-beta.1` or `1.2.3+build`, as Semantic Versioning 2.0.0 reads it. */
export function parseVersion(tag: string): Version | undefined {
  const match =
    /^v?(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?(?:\+[0-9A-Za-z.-]+)?$/.exec(
      tag,
    )
  if (!match) return undefined
  return {
    core: [Number(match[1]), Number(match[2]), Number(match[3])],
    pre: match[4] ? match[4].split(".") : [],
  }
}

const numeric = (id: string) => /^\d+$/.test(id)

function compareIds(a: string, b: string): number {
  if (numeric(a) && numeric(b)) return Number(a) - Number(b)
  if (numeric(a)) return -1
  if (numeric(b)) return 1
  return a < b ? -1 : a > b ? 1 : 0
}

/** SemVer 2.0.0 precedence, section 11: a pre-release comes before its normal version. */
export function compareVersions(a: Version, b: Version): number {
  for (let i = 0; i < 3; i++) {
    const order = a.core[i]! - b.core[i]!
    if (order !== 0) return order
  }
  if (a.pre.length === 0 || b.pre.length === 0) return b.pre.length - a.pre.length
  for (let i = 0; i < Math.min(a.pre.length, b.pre.length); i++) {
    const order = compareIds(a.pre[i]!, b.pre[i]!)
    if (order !== 0) return order
  }
  return a.pre.length - b.pre.length
}

/** The published releases in GitHub's answer, with the tag's `v` dropped. */
export function publishedReleases(answer: unknown): Release[] {
  if (!Array.isArray(answer)) return []
  return answer.flatMap((item: unknown) => {
    if (typeof item !== "object" || item === null) return []
    const r = item as Record<string, unknown>
    if (r.draft === true) return []
    if (typeof r.tag_name !== "string" || typeof r.html_url !== "string") return []
    const version = r.tag_name.replace(/^v/, "")
    if (!parseVersion(version)) return []
    const body = typeof r.body === "string" ? r.body : ""
    return [
      {
        version,
        url: r.html_url,
        prerelease: r.prerelease === true,
        security: body.split(/\r?\n/).some((line) => line.trim() === "### Security"),
      },
    ]
  })
}

/** The newest normal release when it is newer than `running`, or the newest pre-release when
 * `running` is one, with the newer normal ones that fix vulnerabilities; nothing without one. */
export function findNewer(releases: Release[], running: string): Newer | undefined {
  const current = parseVersion(running)
  if (!current) return undefined
  const newer = releases
    .filter((release) => !release.prerelease || current.pre.length > 0)
    .map((release) => ({ release, version: parseVersion(release.version)! }))
    .filter((r) => compareVersions(r.version, current) > 0)
    .sort((a, b) => compareVersions(b.version, a.version))
    .map((r) => r.release)
  const latest = newer[0]
  if (!latest) return undefined
  return { latest, security: newer.filter((r) => r.security && !r.prerelease) }
}

/** `findNewer` for a cluster that has update checking on; cached releases say nothing once it is off. */
export function newerFor(
  zeabur: { version: string; updateCheck: boolean } | undefined,
  checked: Release[] | undefined,
): Newer | undefined {
  return zeabur?.updateCheck && checked ? findNewer(checked, zeabur.version) : undefined
}

export interface Storage {
  load(key: string): string | null
  save(key: string, value: string): void
}

function readCache(storage: Storage): Cached | undefined {
  try {
    const value: unknown = JSON.parse(storage.load(CACHE_KEY) ?? "null")
    if (typeof value !== "object" || value === null) return undefined
    const cached = value as Partial<Cached>
    if (typeof cached.checkedAt !== "number" || !Array.isArray(cached.releases)) return undefined
    return {
      checkedAt: cached.checkedAt,
      releases: cached.releases,
      limitedUntil: typeof cached.limitedUntil === "number" ? cached.limitedUntil : undefined,
    }
  } catch {
    return undefined
  }
}

type Fetch = (url: string, init: RequestInit) => Promise<Response>

/** The releases from GitHub, kept in this browser for the next page. A failed request throws,
 * so the last answer stands. People behind one address share GitHub's hourly limit, so once
 * GitHub says it is used up, the last answer stands with no request until GitHub's reset. */
export async function releases(fetcher: Fetch, storage: Storage, now: number): Promise<Release[]> {
  const cached = readCache(storage)
  if (cached?.limitedUntil !== undefined && now < cached.limitedUntil) return cached.releases
  const controller = new AbortController()
  const timer = setTimeout(() => controller.abort(), CHECK_TIMEOUT)
  try {
    const response = await fetcher(RELEASES_URL, {
      credentials: "omit",
      referrerPolicy: "no-referrer",
      signal: controller.signal,
    })
    const reset = Number(response.headers.get("x-ratelimit-reset"))
    if (response.headers.get("x-ratelimit-remaining") === "0" && reset > 0)
      storage.save(
        CACHE_KEY,
        JSON.stringify({
          checkedAt: cached?.checkedAt ?? 0,
          releases: cached?.releases ?? [],
          limitedUntil: reset * 1000,
        } satisfies Cached),
      )
    if (!response.ok) throw new Error(`GitHub answered ${response.status}`)
    const fresh = publishedReleases(await response.json())
    storage.save(CACHE_KEY, JSON.stringify({ checkedAt: now, releases: fresh } satisfies Cached))
    return fresh
  } finally {
    clearTimeout(timer)
  }
}

/** The releases as a query: the ones this browser read last show at once, and each page that
 * opens asks GitHub again in the background, as does a page left open, every hour. */
export function updateCheckQuery(fetcher: Fetch, storage: Storage, enabled: boolean) {
  return queryOptions({
    queryKey: ["update-check"],
    queryFn: async () => await releases(fetcher, storage, Date.now()),
    enabled,
    initialData: () => readCache(storage)?.releases,
    // Older than any staleTime, so the first page of a visit always asks again.
    initialDataUpdatedAt: 0,
    staleTime: CHECK_EVERY,
    refetchInterval: CHECK_EVERY,
    retry: false,
  })
}
