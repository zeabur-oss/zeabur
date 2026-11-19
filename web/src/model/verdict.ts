// A service's verdict: the server's, with what only the Overview reads added. A rollout card or a
// public domain that fails turns a running service into a deploying or unreachable one, and the
// reach line comes from each domain's trace and the browser's own check.
import type { Schemas } from "@/api/client"
import type { BrowserCheck } from "@/model/browser-check"
import type { Overview } from "@/model/overview"
import type { Health, Service } from "@/model/service"
import { severity } from "@/model/severity"

type Route = Schemas.RouteTraceView

/** What is wrong, in a few words. */
export type Trouble = Schemas.Trouble | { as: "unreachable"; domain: string }

export type Headline =
  | Exclude<Schemas.Headline, { as: "problem" }>
  | { as: "problem"; trouble: Trouble }

/** Whether people reach the public domains, from the cluster's trace or the person's browser;
 * a domain not set up yet says so in the words Networking uses. */
export interface Reach {
  as: "reachable" | "unreachable" | "unknown" | "invalid" | "elsewhere" | "issuing"
  /** Only the browser saw it, so it holds for this network. */
  browser: boolean
  domains: string[]
  /** Networking shows a DNS record to add for one of them. */
  record?: boolean
}

export interface Verdict {
  tone: Health
  headline: Headline
  reach?: Reach
}

const isPublic = (route: Route) => route.kind === "domain" && !route.address.includes("*")

type Seen = { domain: string; as: Reach["as"]; browser: boolean; record: boolean }

function seenOf(route: Route, checks: (address: string) => BrowserCheck | undefined): Seen {
  const domain = route.address
  const check = route.domain ?? undefined
  const record = Boolean(check && check.dns !== "pointsHere" && check.target.length > 0)
  const said = (as: Reach["as"], browser = false): Seen => ({ domain, as, browser, record })
  if (route.status === "ok") return said("reachable")
  const setup = check?.setup
  if (setup === "invalid") return said("invalid")
  if (setup === "pending") return said("issuing")
  if (route.status === "failed") return said("unreachable")
  const browser = checks(domain)?.state
  if (browser === "reachable") return said("reachable", true)
  if (setup === "elsewhere") return said("elsewhere")
  return browser === "unreachable" ? said("unreachable", true) : said("unknown")
}

/** The worst first: a domain to set up, then one nobody reaches, then one still waiting. */
const reachOrder: Reach["as"][] = [
  "invalid",
  "unreachable",
  "elsewhere",
  "issuing",
  "unknown",
  "reachable",
]

function reachOf(
  routes: Route[],
  waiting: Overview["waiting"],
  checks: (address: string) => BrowserCheck | undefined,
): Reach | undefined {
  const seen = routes
    .filter((r) => isPublic(r) && !waiting.has(r.address))
    .map((r) => seenOf(r, checks))
  const as = reachOrder.find((as) => seen.some((s) => s.as === as))
  if (!as) return undefined
  const these = seen.filter((s) => s.as === as)
  const reach: Reach = {
    as,
    browser: as === "reachable" ? these.some((s) => s.browser) : these.every((s) => s.browser),
    domains: these.map((s) => s.domain),
  }
  if (as !== "reachable" && these.some((s) => s.record)) reach.record = true
  return reach
}

export function verdictOf({
  verdict,
  said,
  routes,
  checks,
}: {
  verdict: Service["verdict"]
  said: Pick<Overview, "rollout" | "waiting">
  routes: Route[]
  checks: (address: string) => BrowserCheck | undefined
}): Verdict {
  const { tone, headline } = verdict ?? { tone: "idle", headline: { as: "stopped" } }
  if (headline.as === "stopped" || headline.as === "status") return { tone, headline }
  const reach = reachOf(routes, said.waiting, checks)
  if (headline.as !== "running") return { tone, headline, reach }
  if (said.rollout.as === "card") return { tone: "progress", headline: { as: "deploying" }, reach }
  const failed = routes.find((r) => isPublic(r) && r.status === "failed")
  if (failed)
    return {
      tone: severity.unreachable,
      headline: { as: "problem", trouble: { as: "unreachable", domain: failed.address } },
      reach: reach?.as === "unreachable" ? undefined : reach,
    }
  return { tone, headline, reach }
}

/** The domains a reach line names before counting the rest. */
export const DOMAINS_NAMED = 2
