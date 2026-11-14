// A built container whose app does not listen on the port its readiness check uses: the check is
// refused on that port long after the container started, and its image may declare the port the
// app does listen on. Everything is read from the container the diagnosis names.
import type { Schemas } from "@/api/client"
import { builtContainers, type Build } from "@/model/builds"
import { imageDeclared } from "@/model/private-address"
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

/** The port the diagnosed container's readiness check was refused on, when the connection was
 * refused. */
function refusedPort(d: Diagnosis): number | undefined {
  if (d.kind !== "probeFailure" || d.cause.key !== "diagnosis.probeFailure.cause.readiness")
    return undefined
  const port = /:(\d+): connect: connection refused/.exec(d.cause.params?.output ?? "")?.[1]
  return port === undefined ? undefined : Number(port)
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
  for (const diagnosis of service.diagnoses ?? []) {
    const container = diagnosis.container
    const port = refusedPort(diagnosis)
    if (container === undefined || port === undefined) continue
    if (!builtContainers(service, builds).some((b) => b.container === container)) continue
    if (!stillNotListening(service, diagnosis, now)) continue
    const declared = imageDeclared(service, builds).find((d) => d.container === container)
    const to = declared?.ports[0]
    const offer =
      to === undefined || declared?.ports.includes(port) ? undefined : { container, from: port, to }
    return { diagnosis, port, offer }
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
