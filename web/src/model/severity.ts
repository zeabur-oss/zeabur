// How loud each state is, by one rule (zeabur-notes, "Severity colours"): red when it is not
// doing its job and will not recover without someone acting, amber when it still serves but needs
// attention, progress while it is on its way. A service's own health comes from the server
// (`crates/zeabur-graph/src/health.rs`) by the same rule. A row has one mark: pill and dot share
// the tone.
import { healthRank, type Health } from "@/model/service"

export const severity = {
  runFailed: "fail",
  crashing: "fail",
  imagePull: "fail",
  unschedulable: "fail",
  noneReady: "fail",
  unreachable: "fail",
  certificateFailed: "fail",
  nodeNotReady: "fail",
  errorLine: "fail",

  warningEvent: "warn",
  restarted: "warn",
  fewerReady: "warn",
  dnsElsewhere: "warn",
  overReservation: "warn",
  warningLine: "warn",

  deploying: "progress",
  issuingCertificate: "progress",
  scheduling: "progress",
} as const satisfies Record<string, Health>

/** How long a pod may wait for a node before waiting reads as a problem. */
export const SCHEDULING_GRACE_MS = 5 * 60_000

/** The worst of several; `ok` for none. */
export function worstOf(healths: Iterable<Health>): Health {
  let worst: Health = "ok"
  for (const health of healths) if (healthRank[health] < healthRank[worst]) worst = health
  return worst
}

/** A problem's mark: its service's tone while that still serves, else red. */
export const troubleTone = (health: Health): "fail" | "warn" =>
  health === "warn" ? "warn" : "fail"

/** Replicas ready out of those wanted. */
export function replicasHealth(ready: number, wanted: number): Health {
  if (wanted <= 0) return "idle"
  if (ready <= 0) return severity.noneReady
  return ready < wanted ? severity.fewerReady : "ok"
}

/** A pod waiting for a node: on its way at first, a problem once it has waited too long. */
export function schedulingHealth(created: string | undefined, now: number): Health {
  const since = created ? Date.parse(created) : Number.NaN
  return now - since < SCHEDULING_GRACE_MS ? severity.scheduling : severity.unschedulable
}

/** Whether people reach a domain, as Overview and Networking say it. */
export const reachHealth = {
  reachable: "ok",
  unreachable: severity.unreachable,
  unknown: "idle",
  invalid: severity.unreachable,
  elsewhere: severity.dnsElsewhere,
  issuing: severity.issuingCertificate,
} as const satisfies Record<string, Health>

/** An activity entry: a release, a write, a Kubernetes Warning event, or a container that ended
 * and was started again. What the service is now is said by its status, not its history. */
export const entryTone = {
  release: "brand",
  write: "neutral",
  warning: severity.warningEvent,
  termination: severity.restarted,
} as const

/** A log line by the level it names. */
export const logTone = {
  error: severity.errorLine,
  warn: severity.warningLine,
  info: undefined,
  debug: undefined,
} as const
