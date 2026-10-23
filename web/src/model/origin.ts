// Where a problem comes from and who can fix it: the diagnosis's `fault`, or for an answer without
// one, its kind and cause.
import type { Schemas } from "@/api/client"

type Diagnosis = Schemas["Diagnosis"]

export type Origin = Schemas["FaultOrigin"]
export type Fixer = Schemas["Fixer"] | "unknown"

export interface ProblemOrigin {
  origin: Origin
  fixer: Fixer
}

export function originOf(d: Diagnosis): ProblemOrigin {
  if (d.fault) return { origin: d.fault.origin, fixer: d.fault.fixer }
  const cause = d.cause.key
  switch (d.kind) {
    case "crashLoop":
    case "runFailed":
      return { origin: "code", fixer: "you" }
    case "outOfMemory":
    case "probeFailure":
    case "unreachable":
    case "configMissing":
      return { origin: "configuration", fixer: "you" }
    case "imagePull":
      return cause.endsWith(".rateLimited") || cause.endsWith(".unreachable")
        ? { origin: "upstream", fixer: "you" }
        : { origin: "configuration", fixer: "you" }
    case "quotaBlocked":
      return { origin: "cluster", fixer: "clusterAdmin" }
    case "podSecurity":
      return { origin: "configuration", fixer: "you" }
    case "unschedulable":
      return cause.endsWith(".cpu") || cause.endsWith(".memory")
        ? { origin: "cluster", fixer: "you" }
        : { origin: "cluster", fixer: "clusterAdmin" }
    case "unexplained":
      return { origin: "unknown", fixer: "unknown" }
  }
}

export const originRank: Record<Origin, number> = {
  cluster: 0,
  component: 1,
  zeabur: 2,
  upstream: 3,
  configuration: 4,
  code: 5,
  unknown: 6,
}
