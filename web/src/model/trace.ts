// The hops a request takes to reach a service, and what is known of each. The API will check each
// hop; until it does, the hops come from the objects that route there and the replicas' readiness.
import type { Endpoint, PublicRoute, Service } from "@/model/service"

export type HopState = "ok" | "fail" | "unknown"

export type Hop =
  | { kind: "dns"; host: string; state: HopState }
  | { kind: "route"; object: string; name: string; https: boolean; state: HopState }
  | { kind: "service"; name: string; port?: string; state: HopState }
  | { kind: "replicas"; ready: number; desired: number; state: HopState }

export interface Trace {
  id: string
  hops: Hop[]
  /** When the API last checked the hops; absent while they are only read from objects. */
  checkedAt?: string
}

function replicasHop(service: Service): Hop {
  const ready = service.pods.filter((p) => p.ready).length
  const desired = service.replicas?.desired ?? service.pods.length
  return { kind: "replicas", ready, desired, state: ready > 0 ? "ok" : "fail" }
}

export function publicTrace(route: PublicRoute, endpoints: Endpoint[], service: Service): Trace {
  const endpoint = endpoints.find((e) => e.name === route.backend)
  const port =
    route.port === undefined ||
    endpoint?.ports.some((p) => String(p.port) === route.port || p.name === route.port)
  return {
    id: `${route.host ?? ""}${route.path ?? ""}/${route.ingress}`,
    hops: [
      ...(route.host ? [{ kind: "dns", host: route.host, state: "unknown" } as const] : []),
      { kind: "route", object: "Ingress", name: route.ingress, https: route.https, state: "ok" },
      {
        kind: "service",
        name: route.backend,
        port: route.port,
        state: endpoint && port ? "ok" : "fail",
      },
      replicasHop(service),
    ],
  }
}

export function privateTrace(endpoint: Endpoint, service: Service): Trace {
  return {
    id: endpoint.name,
    hops: [
      {
        kind: "service",
        name: endpoint.name,
        port: endpoint.ports[0] ? String(endpoint.ports[0].port) : undefined,
        state: "ok",
      },
      replicasHop(service),
    ],
  }
}
