// Where a problem comes from and who can fix it. The API will say so per diagnosis; until it does,
// the kind and cause decide.
import type { Schemas } from "@/api/client"

type Diagnosis = Schemas["Diagnosis"]

/** The app's own code, its settings here, the cluster under it, or something outside both. */
export type Origin = "app" | "config" | "cluster" | "upstream" | "unknown"
/** The person looking, a cluster admin, or no one known. */
export type Fixer = "you" | "admin" | "unknown"

export interface ProblemOrigin {
  origin: Origin
  fixer: Fixer
}

const origins: readonly string[] = ["app", "config", "cluster", "upstream", "unknown"]
const fixers: readonly string[] = ["you", "admin", "unknown"]

export function originOf(d: Diagnosis): ProblemOrigin {
  const given = d as Diagnosis & { origin?: unknown; fixer?: unknown }
  if (typeof given.origin === "string" && origins.includes(given.origin))
    return {
      origin: given.origin as Origin,
      fixer:
        typeof given.fixer === "string" && fixers.includes(given.fixer)
          ? (given.fixer as Fixer)
          : "unknown",
    }
  const cause = d.cause.key
  switch (d.kind) {
    case "crashLoop":
    case "runFailed":
      return { origin: "app", fixer: "you" }
    case "outOfMemory":
    case "probeFailure":
    case "unreachable":
    case "configMissing":
      return { origin: "config", fixer: "you" }
    case "imagePull":
      return cause.endsWith(".rateLimited") || cause.endsWith(".unreachable")
        ? { origin: "upstream", fixer: "you" }
        : { origin: "config", fixer: "you" }
    case "quotaBlocked":
      return { origin: "cluster", fixer: "admin" }
    case "unschedulable":
      return cause.endsWith(".cpu") || cause.endsWith(".memory")
        ? { origin: "cluster", fixer: "you" }
        : { origin: "cluster", fixer: "admin" }
    case "unexplained":
      return { origin: "unknown", fixer: "unknown" }
  }
}

export const originRank: Record<Origin, number> = {
  cluster: 0,
  upstream: 1,
  config: 2,
  app: 3,
  unknown: 4,
}
