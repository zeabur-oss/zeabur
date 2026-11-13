// A built service whose app listens on another port than the service's: its traffic check is
// refused on that port long after the container started, and its image may declare the port the
// app does listen on.
import type { Schemas } from "@/api/client"
import { builtContainers, type Build } from "@/model/builds"
import { imageDeclared, likelyPort } from "@/model/private-address"
import type { Service } from "@/model/service"

type Diagnosis = Schemas.Diagnosis

/** How long an app has to start listening before a refused check counts. */
const BIND_WINDOW_MS = 60_000

/** The image's port to use instead of the service's: the lowest it declares, when it declares
 * none the service uses. */
export interface PortOffer {
  container: string
  from: number
  to: number
}

export interface WrongPort {
  diagnosis: Diagnosis
  port: number
  offer?: PortOffer
}

function portOffer(
  service: Service,
  builds: readonly Build[],
  container: string | undefined,
): PortOffer | undefined {
  const from = likelyPort(service)
  if (from === undefined || container === undefined) return undefined
  const declared = imageDeclared(service, builds).find((d) => d.container === container)
  const to = declared?.ports[0]
  if (to === undefined || declared?.ports.includes(from)) return undefined
  return { container, from, to }
}

/** A failing traffic check whose connection to `port` was refused. */
function refusedOn(d: Diagnosis, port: number): boolean {
  if (d.kind !== "probeFailure" || d.cause.key !== "diagnosis.probeFailure.cause.readiness")
    return false
  const output = d.cause.params?.output ?? ""
  return output.includes("connection refused") && output.includes(`:${port}:`)
}

/** Whether the container is still not ready in one of the replicas the diagnosis names, after
 * longer than {@link BIND_WINDOW_MS} to listen. */
function stillNotListening(service: Service, d: Diagnosis, now: number): boolean {
  for (const pod of service.pods) {
    if (!d.pods.includes(pod.name)) continue
    const status = pod.containerStatuses?.find((c) => c.name === d.container)
    if (status?.ready !== false) continue
    const started = status.startedAt ? Date.parse(status.startedAt) : Number.NaN
    if (now - started > BIND_WINDOW_MS) return true
  }
  return false
}

export function wrongPort(
  service: Service,
  builds: readonly Build[],
  now: number,
): WrongPort | undefined {
  if (builtContainers(service, builds).length === 0) return undefined
  const port = likelyPort(service)
  if (port === undefined) return undefined
  for (const diagnosis of service.diagnoses ?? []) {
    if (refusedOn(diagnosis, port) && stillNotListening(service, diagnosis, now))
      return { diagnosis, port, offer: portOffer(service, builds, diagnosis.container) }
  }
  return undefined
}

export const sameDiagnosis = (a: Diagnosis, b: Diagnosis) =>
  a.kind === b.kind && a.container === b.container && a.cause.key === b.cause.key

/** The change that moves the service to the port its image declares. */
export const portChange = (service: Service, offer: PortOffer): Schemas.Change => ({
  op: "port",
  workload: { kind: service.kind, name: service.name },
  container: offer.container,
  from: offer.from,
  to: offer.to,
})
