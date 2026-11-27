import type { Schemas } from "@/api/client"
import type { Service } from "@/model/service"

/** The groups the project's services are in by the group label, by name. */
export const groupNames = (services: readonly Service[]): string[] =>
  [...new Set(services.filter((s) => s.grouped).map((s) => s.group))].toSorted((a, b) =>
    a.localeCompare(b),
  )

/** A label value Kubernetes accepts: at most 63 letters, digits, `-`, `_` or `.`, starting and
 * ending with a letter or digit. */
export const validGroup = (name: string) =>
  name.length <= 63 && /^[A-Za-z0-9]([-A-Za-z0-9_.]*[A-Za-z0-9])?$/.test(name)

/** The change that puts `service` in the group typed, or takes it out when empty; none when it is
 * there already or the name is not valid. */
export function groupChange(service: Service, typed: string): Schemas.Change | undefined {
  const group = typed.trim()
  const current = service.grouped ? service.group : ""
  if (group === current || (group && !validGroup(group))) return undefined
  const workload = { kind: service.kind, name: service.name }
  return group ? { op: "group", workload, group } : { op: "group", workload }
}
