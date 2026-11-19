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
  errorLine: "fail",

  warningEvent: "warn",
  restarted: "warn",
  fewerReady: "warn",
  dnsElsewhere: "warn",
  warningLine: "warn",

  deploying: "progress",
  issuingCertificate: "progress",
  scheduling: "progress",
} as const satisfies Record<string, Health>

/** The worst of several; `ok` for none. */
export function worstOf(healths: Iterable<Health>): Health {
  let worst: Health = "ok"
  for (const health of healths) if (healthRank[health] < healthRank[worst]) worst = health
  return worst
}

/** A problem's mark: its service's tone while that still serves, else red. */
export const troubleTone = (health: Health): "fail" | "warn" =>
  health === "warn" ? "warn" : "fail"

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

/** A timeline entry's warning variant (`crashLoop`, `unschedulable.claimMissing`), from its key. */
export const timelineVariant = (key: string) => key.replace(/^timeline\.warning\./, "")

/**
 * A Warning event is amber unless it says the service cannot work until someone acts: it keeps
 * crashing, cannot pull its image, or cannot be scheduled past the allowance, which the
 * service's own health already says (`health`, now).
 */
export function warningTone(variant: string | null | undefined, health?: Health): "fail" | "warn" {
  if (variant === "crashLoop") return severity.crashing
  if (variant === "imagePull") return severity.imagePull
  if (variant?.startsWith("unschedulable") && health === "fail") return severity.unschedulable
  return severity.warningEvent
}

/** An activity entry's tone: a warning by what it says, a container that ended by whether its
 * service is failing now (it keeps crashing) or runs again. */
export function activityTone(
  entry: { kind: keyof typeof entryTone; message: { key: string } },
  health?: Health,
): "fail" | "warn" | "brand" | "neutral" {
  if (entry.kind === "warning") return warningTone(timelineVariant(entry.message.key), health)
  if (entry.kind === "termination" && health === "fail") return severity.crashing
  return entryTone[entry.kind]
}

/** A log line by the level it names. */
export const logTone = {
  error: severity.errorLine,
  warn: severity.warningLine,
  info: undefined,
  debug: undefined,
} as const
