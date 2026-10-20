// Search parameters of the project and service pages. Everything a person picks — tab, filter,
// sort, selected service or pod — is in the URL, so a link shows the same view and Back works.
import { ranges, splits, type Range, type Split } from "@/model/usage"

const serviceTabs = [
  "overview",
  "logs",
  "terminal",
  "variables",
  "metrics",
  "networking",
  "storage",
  "settings",
  "events",
  "releases",
  "yaml",
] as const
export type ServiceTab = (typeof serviceTabs)[number]

const str = (value: unknown) => (typeof value === "string" && value ? value : undefined)
const oneOf = <T extends string>(values: readonly T[], value: unknown): T | undefined =>
  typeof value === "string" && (values as readonly string[]).includes(value)
    ? (value as T)
    : undefined

export interface ServiceSearch {
  tab?: ServiceTab
  /** The pod the Logs and Terminal tabs read, or the Events tab filters on. */
  pod?: string
  /** How far back the Overview's history reaches (`1h` when absent). */
  range?: Range
  /** The history per replica (when absent) or summed. */
  split?: Split
}

export function validateServiceSearch(search: Record<string, unknown>): ServiceSearch {
  return {
    tab: oneOf(serviceTabs, search.tab),
    pod: str(search.pod),
    range: oneOf(ranges, search.range),
    split: oneOf(splits, search.split),
  }
}

export interface OverviewSearch {
  /** Show every event instead of warnings only. */
  events?: "all"
}

export function validateOverviewSearch(search: Record<string, unknown>): OverviewSearch {
  return { events: search.events === "all" ? "all" : undefined }
}

export interface ArchitectureSearch {
  /** `Kind/name` of the service in the quick look. */
  service?: string
}

export function validateArchitectureSearch(search: Record<string, unknown>): ArchitectureSearch {
  return { service: str(search.service) }
}

export interface ObjectsSearch {
  q?: string
  /** `group/version/resource/name` of the object whose YAML is open. */
  object?: string
  /** Also list the kinds controllers create on their own. */
  generated?: boolean
  /** Also show the kinds that were read and hold nothing. */
  empty?: boolean
}

export function validateObjectsSearch(search: Record<string, unknown>): ObjectsSearch {
  return {
    q: str(search.q),
    object: str(search.object),
    generated: search.generated === true || search.generated === "true" ? true : undefined,
    empty: search.empty === true || search.empty === "true" ? true : undefined,
  }
}

export const profiles = ["single-node", "multi-node"] as const

export interface ComponentsSearch {
  /** `capability/id` of the component whose install texts are open. */
  component?: string
  /** The profile those texts are for; the cluster's own by default. */
  profile?: (typeof profiles)[number]
}

export function validateComponentsSearch(search: Record<string, unknown>): ComponentsSearch {
  return { component: str(search.component), profile: oneOf(profiles, search.profile) }
}

const statusFilters = ["attention", "healthy"] as const
const sortKeys = ["name", "status", "ready", "restarts", "kind"] as const
export type SortKey = (typeof sortKeys)[number]

export interface ServicesSearch {
  q?: string
  status?: (typeof statusFilters)[number]
  kind?: string
  sort?: SortKey
  desc?: boolean
}

export function validateServicesSearch(search: Record<string, unknown>): ServicesSearch {
  return {
    q: str(search.q),
    status: oneOf(statusFilters, search.status),
    kind: str(search.kind),
    sort: oneOf(sortKeys, search.sort),
    desc: search.desc === true || search.desc === "true" ? true : undefined,
  }
}
