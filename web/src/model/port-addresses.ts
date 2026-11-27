import type { Schemas } from "@/api/client"
import type { Endpoint } from "@/model/service"

type Port = Schemas.Port
export type AddressKind = Schemas.PortAddressKind

/** What a row of a kind says while the Service type gives that kind but the server lists no
 * address yet: a node port whose nodes are unknown, or a load balancer that has not answered. */
export type Pending = { message: "addr.nodePortOnly" | "ports.waiting"; port: number }

interface KindRule {
  /** Whether addresses of this kind are reachable from outside the cluster. */
  outside: boolean
  pending: (e: Endpoint, p: Port) => Pending | undefined
}

const none = () => undefined

/** Every kind of address, in the order a port lists them. A new kind of Service address is one
 * more entry here, and one more label in `addr.<kind>`. */
const KINDS: Readonly<Record<AddressKind, KindRule>> = {
  project: { outside: false, pending: none },
  cluster: { outside: false, pending: none },
  node: {
    outside: true,
    pending: (_, p) =>
      p.nodePort ? { message: "addr.nodePortOnly", port: p.nodePort } : undefined,
  },
  loadBalancer: {
    outside: true,
    pending: (e, p) =>
      e.type === "LoadBalancer" ? { message: "ports.waiting", port: p.port } : undefined,
  },
}

/** One kind of address of a port: its addresses, or what stands in while there are none. */
export interface AddressRow {
  kind: AddressKind
  addresses: string[]
  pending?: Pending
}

/** The rows of a port, one per kind that has an address or stands in for one. */
export function addressRows(e: Endpoint, p: Port): AddressRow[] {
  return (Object.keys(KINDS) as AddressKind[]).flatMap((kind) => {
    const addresses = (p.addresses ?? []).filter((a) => a.kind === kind).map((a) => a.address)
    const pending = addresses.length === 0 ? KINDS[kind].pending(e, p) : undefined
    return addresses.length > 0 || pending ? [{ kind, addresses, pending }] : []
  })
}

/** Whether the internet can reach the Service, as far as its type opens it: a port with an
 * address, or a stand-in for one, of a kind outside the cluster. */
export const outside = (e: Endpoint): boolean =>
  e.ports.some((p) => addressRows(e, p).some((row) => KINDS[row.kind].outside))

/** The Service type as Kubernetes names it; a `ClusterIP` Service without a cluster IP is
 * headless. */
export const typeLabel = (e: Endpoint): string =>
  e.clusterIP === "None" ? "Headless" : (e.type ?? "ClusterIP")

/** Whether any port of `endpoints` has a node port, and so needs the nodes for its addresses. */
export const hasNodePort = (endpoints: Endpoint[]): boolean =>
  endpoints.some((e) => e.ports.some((p) => p.nodePort))
