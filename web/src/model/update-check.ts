/** The update check: the running version against the releases the server read from GitHub. */

import { queryOptions } from "@tanstack/react-query"

import type { Schemas } from "@/api/client"
import * as zeabur from "@/api/zeabur"

/** How often a page left open asks again. */
export const CHECK_EVERY = 60 * 60_000

export type Release = Schemas.ZeaburRelease

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

/** The newest normal release when it is newer than `running`, or the newest pre-release when
 * `running` is one, with the newer normal ones that fix vulnerabilities; nothing without one. */
export function findNewer(releases: Release[], running: string): Newer | undefined {
  const current = parseVersion(running)
  if (!current) return undefined
  const newer = releases
    .filter((release) => !release.prerelease || current.pre.length > 0)
    .flatMap((release) => {
      const version = parseVersion(release.version)
      return version ? [{ release, version }] : []
    })
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

/** Zeabur's releases as the server read them from GitHub. A page that opens asks the server, as
 * does a page left open, every hour; the server asks GitHub at most once an hour. */
export const releasesQuery = (enabled: boolean) =>
  queryOptions({
    queryKey: ["cluster", "zeabur", "releases"],
    queryFn: async ({ signal }) => (await zeabur.getZeaburReleases({ signal })).items,
    enabled,
    staleTime: CHECK_EVERY,
    refetchInterval: CHECK_EVERY,
    retry: false,
  })
