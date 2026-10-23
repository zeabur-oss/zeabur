// One plain verdict for a service, from what the Overview already reads: its status, the causes
// the diagnosis found, the rollout, each domain's trace and the browser's own check.
import type { Schemas } from "@/api/client"
import type { BrowserCheck } from "@/model/browser-check"
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

/** Whether people reach the public domains, from the cluster's trace or the person's browser. */
export interface Reach {
  as: "reachable" | "unreachable" | "unknown"
  /** Only the browser saw it, so it holds for this network. */
  browser: boolean
  domains: string[]
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

function reachOf(
  routes: Route[],
  waiting: Overview["waiting"],
  checks: (address: string) => BrowserCheck | undefined,
): Reach | undefined {
  const domains = routes.filter((r) => isPublic(r) && !waiting.has(r.address))
  if (domains.length === 0) return undefined
  const seen = domains.map((route) => {
    if (route.status !== "unknown")
      return { domain: route.address, reached: route.status === "ok", browser: false }
    const check = checks(route.address)?.state
    return {
      domain: route.address,
      reached: check === "reachable" ? true : check === "unreachable" ? false : undefined,
      browser: true,
    }
  })
  const down = seen.filter((s) => s.reached === false)
  if (down.length > 0)
    return {
      as: "unreachable",
      browser: down.every((s) => s.browser),
      domains: down.map((s) => s.domain),
    }
  const open = seen.filter((s) => s.reached === undefined)
  if (open.length > 0) return { as: "unknown", browser: false, domains: open.map((s) => s.domain) }
  return {
    as: "reachable",
    browser: seen.some((s) => s.browser),
    domains: seen.map((s) => s.domain),
  }
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
