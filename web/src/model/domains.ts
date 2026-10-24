import type { Schemas } from "@/api/client"
import type { PublicRoute } from "@/model/service"

/** Distinct host names, and how many of them serve plain HTTP. */
export function domainsLead(routes: PublicRoute[]): { hosts: number; plain: number } {
  const hosts = new Map<string, boolean>()
  for (const route of routes)
    if (route.host) hosts.set(route.host, (hosts.get(route.host) ?? false) || route.https === false)
  return { hosts: hosts.size, plain: [...hosts.values()].filter(Boolean).length }
}

const isPlain = (route: PublicRoute) => Boolean(route.host) && route.https === false

/** Hosts without HTTPS first; otherwise in the order the routes list them. */
export const plainFirst = (routes: PublicRoute[]) =>
  routes.toSorted((a, b) => Number(isPlain(b)) - Number(isPlain(a)))

export function filterRoutes(routes: PublicRoute[], query: string): PublicRoute[] {
  const needle = query.trim().toLowerCase()
  if (!needle) return routes
  return routes.filter((route) =>
    [route.host, route.path].some((text) => text?.toLowerCase().includes(needle)),
  )
}

type DomainCheck = Schemas["DomainCheck"]

export interface DnsRecord {
  type: "A" | "AAAA" | "CNAME"
  name: string
  value: string
}

const isIp = (address: string) => /^\d{1,3}(\.\d{1,3}){3}$/.test(address) || address.includes(":")

/** The records that send `name` to `target`: an A or AAAA record per address, or one CNAME to
 * the first host name, since a name with a CNAME can have no other record. */
export function recordsFor(name: string, target: readonly string[]): DnsRecord[] {
  const ips = target.filter(isIp)
  if (ips.length > 0)
    return ips.map((value) => ({ type: value.includes(":") ? "AAAA" : "A", name, value }))
  const host = target[0]
  return host ? [{ type: "CNAME", name, value: host }] : []
}

/** The wildcard of `suffix` that covers `host`: one label under it, as a DNS wildcard matches. */
export function wildcardFor(host: string, suffix: string | undefined): string | undefined {
  if (!suffix) return undefined
  const label = host.endsWith(`.${suffix}`) ? host.slice(0, -(suffix.length + 1)) : undefined
  return label && !label.includes(".") ? `*.${suffix}` : undefined
}

/** What a domain's DNS needs, Vercel-style: fine, or the records to add and why. */
export type DnsSetup =
  | { as: "valid" }
  | { as: "missing"; records: DnsRecord[] }
  | { as: "elsewhere"; records: DnsRecord[]; resolved: string[] }
  | { as: "unknown"; records: DnsRecord[] }

export function dnsSetup(host: string, check: DomainCheck, suffix?: string): DnsSetup {
  if (check.dns === "pointsHere") return { as: "valid" }
  const records = recordsFor(wildcardFor(host, suffix) ?? host, check.target)
  if (check.dns === "missing") return { as: "missing", records }
  if (check.dns === "elsewhere") return { as: "elsewhere", records, resolved: check.resolved ?? [] }
  return { as: "unknown", records }
}

/** Valid only when DNS points here and the certificate, if any, is issued or not cert-manager's;
 * invalid when a record is missing or issuing failed. Another address may be a CDN in front. */
export function configuration(
  check: DomainCheck,
): "valid" | "invalid" | "pending" | "elsewhere" | "unknown" {
  const certificate = check.certificate?.state
  if (check.dns === "missing" || certificate === "failed") return "invalid"
  if (certificate === "issuing") return "pending"
  if (check.dns === "pointsHere") return "valid"
  return check.dns === "elsewhere" ? "elsewhere" : "unknown"
}

/** Whether a domain has something to do: a record to set or a certificate not issued yet. */
export const needsSetup = (check: DomainCheck) =>
  check.dns !== "pointsHere" ||
  check.certificate?.state === "failed" ||
  check.certificate?.state === "issuing"
