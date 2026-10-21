// Incidents grouped by what they need: 25 nodes that stopped reporting are one line with one fix.
import type { Schemas } from "@/api/client"

type Incident = Schemas["Incident"]
type Affected = Schemas["AffectedService"]

export const serviceKey = (s: { project: string; kind: string; name: string }) =>
  `${s.project}/${s.kind}/${s.name}`

/** Incidents with the same cause and the same fix, such as 25 nodes that stopped reporting. */
export interface IncidentGroup {
  key: string
  incidents: Incident[]
  /** Every service they take down, once. */
  services: Affected[]
  projects: number
  domains: number
}

const groupKey = (incident: Incident) =>
  incident.kind === "nodeNotReady"
    ? `${incident.kind}/${incident.cause.params?.status === "Unknown" ? "Unknown" : "NotReady"}`
    : incident.id

/** One group per kind of cause; components stay one each, as each has its own fix. Largest
 * first, then in the order they came. */
export function groupIncidents(incidents: readonly Incident[]): IncidentGroup[] {
  const groups = new Map<string, Incident[]>()
  for (const incident of incidents) {
    const k = groupKey(incident)
    groups.set(k, [...(groups.get(k) ?? []), incident])
  }
  return [...groups.entries()]
    .map(([k, list]) => {
      const services = new Map<string, Affected>()
      for (const s of list.flatMap((i) => i.services)) services.set(serviceKey(s), s)
      const affected = [...services.values()]
      return {
        key: k,
        incidents: list,
        services: affected,
        projects: new Set(affected.map((s) => s.project)).size,
        domains: new Set(list.flatMap((i) => i.domains)).size,
      }
    })
    .toSorted((a, b) => b.incidents.length - a.incidents.length)
}
