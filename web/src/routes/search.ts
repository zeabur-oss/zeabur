import { ranges, splits, type Range, type Split } from "@/model/usage"

export const serviceSections = ["status", "changes", "configure", "inspect"] as const
export type ServiceSection = (typeof serviceSections)[number]

const inspectViews = ["logs", "terminal", "usage", "yaml", "events"] as const
export type InspectView = (typeof inspectViews)[number]

const configureParts = ["variables", "networking", "storage", "compute", "health-checks"] as const
export type ConfigurePart = (typeof configureParts)[number]

/** The names `?tab=` had when every part of a service was a tab of its own. */
const formerTabs = ["overview", "releases", "metrics", "settings"] as const

/** A place on a service's page: a section, a view of Inspect or a part of Configure. */
export type ServicePlace =
  | ServiceSection
  | InspectView
  | ConfigurePart
  | (typeof formerTabs)[number]

const places: readonly ServicePlace[] = [
  ...serviceSections,
  ...inspectViews,
  ...configureParts,
  ...formerTabs,
]

/** Where a place is: its section, the view of Inspect, and the part of Configure (the hash). */
export function servicePlace(place?: ServicePlace): {
  tab?: ServiceSection
  view?: InspectView
  hash?: ConfigurePart
} {
  switch (place) {
    case undefined:
    case "overview":
    case "status":
      return {}
    case "changes":
    case "releases":
      return { tab: "changes" }
    case "configure":
    case "inspect":
      return { tab: place }
    case "settings":
      return { tab: "configure", hash: "compute" }
    case "metrics":
      return { tab: "inspect", view: "usage" }
    case "variables":
    case "networking":
    case "storage":
    case "compute":
    case "health-checks":
      return { tab: "configure", hash: place }
    case "logs":
    case "terminal":
    case "usage":
    case "yaml":
    case "events":
      return { tab: "inspect", view: place }
  }
}

const str = (value: unknown) => (typeof value === "string" && value ? value : undefined)
const oneOf = <T extends string>(values: readonly T[], value: unknown): T | undefined =>
  typeof value === "string" && (values as readonly string[]).includes(value)
    ? (value as T)
    : undefined

/** A `?tab=` from before the four sections, to redirect from. */
export const formerPlace = (tab: unknown): ServicePlace | undefined => {
  const place = oneOf(places, tab)
  return place && !(serviceSections as readonly string[]).includes(place) ? place : undefined
}

export interface ServiceSearch {
  /** Status when absent. */
  tab?: ServiceSection
  /** Inspect's view; Logs when absent. */
  view?: InspectView
  pod?: string
  /** `1h` when absent. */
  range?: Range
  /** Per replica when absent. */
  split?: Split
}

export function validateServiceSearch(search: Record<string, unknown>): ServiceSearch {
  const place = servicePlace(oneOf(places, search.tab))
  return {
    tab: place.tab,
    view: oneOf(inspectViews, search.view) ?? place.view,
    pod: str(search.pod),
    range: oneOf(ranges, search.range),
    split: oneOf(splits, search.split),
  }
}

export interface ObjectsSearch {
  q?: string
  /** `group/version/resource/name` of the object whose YAML is open. */
  object?: string
  generated?: boolean
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
  /** The cluster's own when absent. */
  profile?: (typeof profiles)[number]
}

export function validateComponentsSearch(search: Record<string, unknown>): ComponentsSearch {
  return { component: str(search.component), profile: oneOf(profiles, search.profile) }
}

const statusFilters = ["attention", "healthy"] as const
const sortKeys = ["name", "status", "ready", "restarts", "kind"] as const
export type SortKey = (typeof sortKeys)[number]
const projectViews = ["map", "list"] as const
export type ProjectView = (typeof projectViews)[number]

export interface ProjectSearch {
  /** The one last chosen when absent; a list for a project too big to draw. */
  view?: ProjectView
  /** `Kind/name` of the service in the quick look. */
  service?: string
  /** Absent: warnings only. */
  events?: "all"
  q?: string
  status?: (typeof statusFilters)[number]
  kind?: string
  sort?: SortKey
  desc?: boolean
}

export function validateProjectSearch(search: Record<string, unknown>): ProjectSearch {
  return {
    view: oneOf(projectViews, search.view),
    service: str(search.service),
    events: search.events === "all" ? "all" : undefined,
    q: str(search.q),
    status: oneOf(statusFilters, search.status),
    kind: str(search.kind),
    sort: oneOf(sortKeys, search.sort),
    desc: search.desc === true || search.desc === "true" ? true : undefined,
  }
}
