// What a service's Overview says, each cause once: the diagnoses that are causes, what they also
// take down, whether a rollout gets a card of its own, and the warnings no diagnosis explains.
import type { Schemas } from "@/api/client"
import { isWarning, type Event } from "@/model/service"

type Diagnosis = Schemas["Diagnosis"]
type Rollout = Schemas["RolloutProgress"]
type Route = Schemas["RouteTraceView"]

/** A diagnosis that only follows from another one: no ready replica, because none starts. */
const isConsequence = (d: Diagnosis) =>
  d.kind === "unreachable" && d.cause.key.endsWith(".noReadyEndpoints")

/** The kinds that keep a new replica from ever becoming ready, as the API's rollout reads them. */
const stalls = new Set<Diagnosis["kind"]>([
  "crashLoop",
  "outOfMemory",
  "imagePull",
  "unschedulable",
  "quotaBlocked",
  "configMissing",
  "runFailed",
])

/** The event reasons each kind of diagnosis already explains, on the objects it names. */
const explains: Record<Diagnosis["kind"], readonly string[]> = {
  imagePull: ["Failed", "BackOff", "ErrImagePull", "ImagePullBackOff", "InspectFailed"],
  crashLoop: ["BackOff", "Unhealthy", "Killing"],
  outOfMemory: ["BackOff", "Unhealthy", "Killing", "OOMKilling"],
  probeFailure: ["Unhealthy", "Killing", "BackOff"],
  runFailed: ["BackoffLimitExceeded", "DeadlineExceeded", "Failed", "BackOff"],
  unschedulable: ["FailedScheduling"],
  quotaBlocked: ["FailedCreate"],
  configMissing: ["FailedMount", "Failed"],
  unreachable: [],
  unexplained: [],
}

export function explainedBy(event: Event, diagnoses: Diagnosis[]): boolean {
  return diagnoses.some((d) => {
    if (d.evidence.some((o) => o.kind === "Event" && o.name === event.origin.name)) return true
    const about = event.regarding
    if (!about || !explains[d.kind].includes(event.reason)) return false
    if (about.kind === "Pod" && d.pods.includes(about.name)) return true
    return d.evidence.some((o) => o.kind === about.kind && o.name === about.name)
  })
}

/** Why an address can't be reached when the hop that fails is only the replicas' readiness. */
type Waiting = "cause" | "stopped"

type RolloutShown =
  | { as: "none" }
  | { as: "card" }
  /** A line in the cause's card: the release waits on it, and is stuck when it never will. */
  | { as: "line"; stuck: boolean }

export interface Overview {
  /** The diagnoses to show, causes only. */
  causes: Diagnosis[]
  /** Addresses whose only failing hop is that no replica is ready, and why that is. */
  waiting: Map<string, Waiting>
  /** The domains and whether the private address are down because of the first cause. */
  alsoDown: { domains: string[]; private: boolean } | undefined
  rollout: RolloutShown
  /** Warnings no diagnosis explains. */
  warnings: Event[]
}

/** Whether the first hop that fails on the way to `route` is the replicas' readiness. */
const stopsAtReady = (route: Route) =>
  route.status === "failed" && route.hops.find((h) => h.status === "failed")?.step === "ready"

export function overviewOf(
  service: {
    diagnoses?: Diagnosis[] | null
    /** Scaled to zero: no replica is meant to run. */
    stopped: boolean
  },
  rollout: Rollout | undefined,
  routes: Route[],
  events: Event[],
): Overview {
  const diagnoses = service.diagnoses ?? []
  const hasCause = diagnoses.some((d) => !isConsequence(d))
  const causes = hasCause ? diagnoses.filter((d) => !isConsequence(d)) : diagnoses
  const known: Waiting | undefined =
    causes.length > 0 ? "cause" : service.stopped ? "stopped" : undefined
  const waiting = new Map<string, Waiting>()
  if (known) for (const route of routes) if (stopsAtReady(route)) waiting.set(route.address, known)
  const down = routes.filter((r) => waiting.get(r.address) === "cause")
  const alsoDown =
    down.length > 0
      ? {
          domains: down.filter((r) => r.kind === "domain").map((r) => r.address),
          private: down.some((r) => r.kind !== "domain"),
        }
      : undefined
  return {
    causes,
    waiting,
    alsoDown,
    rollout: rolloutShown(rollout, causes),
    warnings: events.filter((e) => isWarning(e) && !explainedBy(e, diagnoses)),
  }
}

function rolloutShown(rollout: Rollout | undefined, causes: Diagnosis[]): RolloutShown {
  if (!rollout || rollout.status === "done" || rollout.desired === 0) return { as: "none" }
  if (causes.length === 0) return { as: "card" }
  // A replica crashing with no release under way is the diagnosis alone, not a stuck rollout.
  const underway =
    rollout.revision != null && (rollout.old > 0 || rollout.updated < rollout.desired)
  if (!underway) return { as: "none" }
  return {
    as: "line",
    stuck: rollout.status === "stuck" || causes.some((d) => stalls.has(d.kind)),
  }
}

/** Plain words for the event reasons people meet most; the reason itself stays beside them. */
const eventWords = {
  BackOff: "retrying",
  Unhealthy: "checkFailed",
  Killing: "stopping",
  Failed: "failed",
  FailedScheduling: "noRoom",
  FailedMount: "mountFailed",
  FailedCreate: "createFailed",
  BackoffLimitExceeded: "gaveUp",
  Evicted: "evicted",
  OOMKilling: "outOfMemory",
} as const

export type EventWord = (typeof eventWords)[keyof typeof eventWords]

export const eventWord = (reason: string | undefined): EventWord | undefined =>
  reason && Object.hasOwn(eventWords, reason)
    ? eventWords[reason as keyof typeof eventWords]
    : undefined

type Gap = Schemas["GapView"]

/** A project's suggestions: each gap on its own, and one row for every service without a traffic check. */
export type Suggestion =
  | { as: "gap"; gap: Gap }
  | { as: "unchecked"; services: { kind: string; name: string }[] }

const uncheckedKeys = new Set(["gap.healthChecks.none", "gap.healthChecks.livenessOnly"])

/** The same advice for many services reads as one low-priority row, after the others. */
export function suggestionsOf(gaps: Gap[]): Suggestion[] {
  const rows: Suggestion[] = []
  const unchecked = new Map<string, { kind: string; name: string }>()
  for (const gap of gaps) {
    if (uncheckedKeys.has(gap.message.key)) {
      const { kind, name } = gap.subject
      unchecked.set(`${kind}/${name}`, { kind, name })
    } else rows.push({ as: "gap", gap })
  }
  if (unchecked.size > 0) rows.push({ as: "unchecked", services: [...unchecked.values()] })
  return rows
}
