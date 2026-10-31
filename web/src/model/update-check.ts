/** The update check: once a day this browser reads Zeabur's releases from GitHub and compares
 * the newest normal one with the running version. Zeabur's server makes no request. */

import { queryOptions } from "@tanstack/react-query"

export const RELEASES_URL = "https://api.github.com/repos/zeabur-oss/zeabur/releases"
export const CHECK_EVERY = 24 * 60 * 60_000
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

function due(cached: Cached | undefined, now: number): boolean {
  return !cached || now < cached.checkedAt || now - cached.checkedAt >= CHECK_EVERY
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
    return { checkedAt: cached.checkedAt, releases: cached.releases }
  } catch {
    return undefined
  }
}

type Fetch = (url: string, init: RequestInit) => Promise<Response>

/** The releases, from this browser's cache when it is less than a day old, else from GitHub. A
 * failed request keeps the last answer and waits a day too. */
export async function releases(fetcher: Fetch, storage: Storage, now: number): Promise<Release[]> {
  const cached = readCache(storage)
  if (cached && !due(cached, now)) return cached.releases
  let fresh = cached?.releases ?? []
  const controller = new AbortController()
  const timer = setTimeout(() => controller.abort(), CHECK_TIMEOUT)
  try {
    const response = await fetcher(RELEASES_URL, {
      credentials: "omit",
      referrerPolicy: "no-referrer",
      signal: controller.signal,
    })
    if (response.ok) fresh = publishedReleases(await response.json())
  } catch {
    // Offline or blocked: the last answer stands.
  } finally {
    clearTimeout(timer)
  }
  storage.save(CACHE_KEY, JSON.stringify({ checkedAt: now, releases: fresh } satisfies Cached))
  return fresh
}

/** The releases as a query that asks again every day, so a page left open still reaches the
 * cache's expiry; `releases` itself decides whether that is a request. */
export function updateCheckQuery(fetcher: Fetch, storage: Storage, enabled: boolean) {
  return queryOptions({
    queryKey: ["update-check"],
    queryFn: async () => await releases(fetcher, storage, Date.now()),
    enabled,
    staleTime: Infinity,
    refetchInterval: CHECK_EVERY,
    retry: false,
  })
}
