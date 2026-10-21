import type { Schemas } from "@/api/client"
import { byHealth, healthRank, needsAttention, statusOf, type Service } from "@/model/service"

type Diagnosis = Schemas["Diagnosis"]
type Sentence = Diagnosis["cause"]

/** A cause's parameters that name the service itself; the rest (an image, a secret) is shared. */
const own = new Set(["container", "service", "pod", "endpoint", "address", "restarts", "route"])

export interface ProblemGroup {
  key: string
  /** Worst first, then by name. */
  services: Service[]
  /** The first service's diagnosis, which the others share up to their own names. */
  diagnosis?: Diagnosis
}

function keyOf(service: Service): string {
  const status = statusOf(service)
  const first = service.diagnoses?.[0]
  if (!first) return `${status.health}/${status.word}/${status.reason ?? ""}`
  const shared = Object.entries(first.cause.params ?? {})
    .filter(([name]) => !own.has(name))
    .toSorted(([a], [b]) => a.localeCompare(b))
    .map(([name, value]) => `${name}=${value}`)
  return `${status.health}/${first.kind}/${first.cause.key}/${shared.join("&")}`
}

/** Services that need attention, one group per diagnosis and cause: worst first, then largest. */
export function groupProblems(services: Service[]): ProblemGroup[] {
  const groups = new Map<string, Service[]>()
  for (const service of services.filter((s) => needsAttention(statusOf(s))).toSorted(byHealth)) {
    const key = keyOf(service)
    groups.set(key, [...(groups.get(key) ?? []), service])
  }
  return [...groups.entries()]
    .map(([key, list]) => ({ key, services: list, diagnosis: list[0]?.diagnoses?.[0] }))
    .toSorted((a, b) => {
      const [sa, sb] = [a.services[0], b.services[0]]
      const rank = (s?: Service) => (s ? healthRank[statusOf(s).health] : 9)
      return (
        rank(sa) - rank(sb) || b.services.length - a.services.length || a.key.localeCompare(b.key)
      )
    })
}

/**
 * The cause said once for every service in a group, with `subject` ("Each one") where it named
 * the container; undefined when the sentence names the service some other way.
 */
export function sharedCause(cause: Sentence, subject: string): Sentence | undefined {
  const params = cause.params ?? {}
  if (!("container" in params)) return undefined
  if (
    Object.keys(params).some((name) => own.has(name) && name !== "container" && name !== "restarts")
  )
    return undefined
  return { ...cause, params: { ...params, container: subject } }
}
