import type { Schemas } from "@/api/client"
import { serviceId, type Service } from "@/model/service"

const current = (service: Service) => (service.grouped ? service.group : "")

/** A label value Kubernetes accepts: at most 63 letters, digits, `-`, `_` or `.`, starting and
 * ending with a letter or digit. */
export const validGroup = (name: string) =>
  name.length <= 63 && /^[A-Za-z0-9]([-A-Za-z0-9_.]*[A-Za-z0-9])?$/.test(name)

/** The change that puts `service` in the group typed, or takes it out when empty; none when it is
 * there already or the name is not valid. */
export function groupChange(service: Service, typed: string): Schemas.Change | undefined {
  const group = typed.trim()
  if (group === current(service) || (group && !validGroup(group))) return undefined
  const workload = { kind: service.kind, name: service.name }
  return group ? { op: "group", workload, group } : { op: "group", workload }
}

/** The group each service is in, by `serviceId`; "" is no group. */
export type Placement = Readonly<Record<string, string>>

export const placementOf = (services: readonly Service[]): Placement =>
  Object.fromEntries(services.map((s) => [serviceId(s), current(s)]))

/** The group `placement` puts the service in; the cluster's when it does not say. */
const groupIn = (placement: Placement, service: Service) =>
  placement[serviceId(service)] ?? current(service)

export interface GroupSection {
  /** "" holds the services in no group. */
  name: string
  services: Service[]
}

/** One section per group by name, then the services in no group. `added` are the groups made
 * here that have no service yet. */
export function groupSections(
  services: readonly Service[],
  placement: Placement,
  added: readonly string[] = [],
): GroupSection[] {
  const at = (s: Service) => groupIn(placement, s)
  const names = [...new Set([...services.map(at), ...added].filter((n) => n !== ""))].toSorted(
    (a, b) => a.localeCompare(b),
  )
  return [...names, ""].map((name) => ({
    name,
    services: services
      .filter((s) => at(s) === name)
      .toSorted((a, b) => a.name.localeCompare(b.name)),
  }))
}

/** One change for each service whose group differs from the cluster's. */
export const groupMoves = (services: readonly Service[], placement: Placement): Schemas.Change[] =>
  services.flatMap((s) => groupChange(s, groupIn(placement, s)) ?? [])
