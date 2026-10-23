// One plain verdict for a service, from what the Overview already reads: its status, the causes
// the diagnosis found, the rollout, each domain's trace and the browser's own check.
import type { Schemas } from "@/api/client"
import type { BrowserCheck } from "@/model/browser-check"
import { configuration } from "@/model/domains"
import type { Overview } from "@/model/overview"
import { reasonWord, type Health, type ReasonWord, type Status } from "@/model/service"

type Diagnosis = Schemas["Diagnosis"]
type Route = Schemas["RouteTraceView"]

/** What is wrong, in a few words. */
export type Trouble =
  | { as: "reason"; word: ReasonWord }
  | { as: "status"; word: Status["word"] }
  | { as: "healthCheck" }
  | { as: "quota" }
  | { as: "unreachable"; domain: string }

export type Headline =
  | { as: "running" }
  | { as: "deploying" }
  | { as: "stopped" }
  | { as: "problem"; trouble: Trouble }
  /** A task's own word: completed, scheduled, paused. */
  | { as: "status"; word: Status["word"] }

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

const kindTrouble: Partial<Record<Diagnosis["kind"], Trouble>> = {
  crashLoop: { as: "reason", word: "crashing" },
  outOfMemory: { as: "reason", word: "outOfMemory" },
  imagePull: { as: "reason", word: "imagePull" },
  configMissing: { as: "reason", word: "configMissing" },
  unschedulable: { as: "reason", word: "waitingForRoom" },
  runFailed: { as: "reason", word: "runFailed" },
  probeFailure: { as: "healthCheck" },
  quotaBlocked: { as: "quota" },
}

function troubleOf(status: Status, causes: Diagnosis[]): Trouble {
  const word = reasonWord(status.reason, status.task ?? false)
  if (word) return { as: "reason", word }
  for (const cause of causes) {
    const trouble = kindTrouble[cause.kind]
    if (trouble) return trouble
  }
  return { as: "status", word: status.word }
}

const isPublic = (route: Route) => route.kind === "domain" && !route.address.includes("*")

type Seen = { domain: string; as: Reach["as"]; browser: boolean; record: boolean }

function seenOf(route: Route, checks: (address: string) => BrowserCheck | undefined): Seen {
  const domain = route.address
  const check = route.domain ?? undefined
  const record = Boolean(check && check.dns !== "pointsHere" && check.target.length > 0)
  const said = (as: Reach["as"], browser = false): Seen => ({ domain, as, browser, record })
  if (route.status === "ok") return said("reachable")
  const config = check ? configuration(check) : undefined
  if (config === "invalid") return said("invalid")
  if (config === "pending") return said("issuing")
  if (route.status === "failed") return said("unreachable")
  const browser = checks(domain)?.state
  if (browser === "reachable") return said("reachable", true)
  if (config === "elsewhere") return said("elsewhere")
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
  status,
  said,
  routes,
  checks,
}: {
  status: Status
  said: Pick<Overview, "causes" | "rollout" | "waiting">
  routes: Route[]
  checks: (address: string) => BrowserCheck | undefined
}): Verdict {
  const reach = reachOf(routes, said.waiting, checks)
  if (status.desired === 0 && !status.task) return { tone: "idle", headline: { as: "stopped" } }
  if (said.causes.length > 0 || status.health === "fail" || status.health === "warn")
    return {
      tone: status.health === "warn" ? "warn" : "fail",
      headline: { as: "problem", trouble: troubleOf(status, said.causes) },
      reach,
    }
  if (status.task || status.health === "idle")
    return { tone: status.health, headline: { as: "status", word: status.word } }
  if (status.health === "progress" || said.rollout.as === "card")
    return { tone: "progress", headline: { as: "deploying" }, reach }
  const failed = routes.find((r) => isPublic(r) && r.status === "failed")
  if (failed)
    return {
      tone: "warn",
      headline: { as: "problem", trouble: { as: "unreachable", domain: failed.address } },
      reach: reach?.as === "unreachable" ? undefined : reach,
    }
  return { tone: "ok", headline: { as: "running" }, reach }
}

/** The domains a reach line names before counting the rest. */
export const DOMAINS_NAMED = 2
