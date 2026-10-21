import { ranges, splits, type Range, type Split } from "@/model/usage"

/** A service's tabs, in their order. */
export const serviceTabs = [
  "overview",
  "logs",
  "deployments",
  "usage",
  "networking",
  "variables",
  "settings",
] as const
export type ServiceTab = (typeof serviceTabs)[number]

/** Views for people who know Kubernetes, each a second view of a tab, in `?mode=`. */
export const serviceModes = { events: "deployments", yaml: "settings" } as const
export type ServiceMode = keyof typeof serviceModes
const modes = Object.keys(serviceModes) as ServiceMode[]

/** The parts of Settings, each an anchor. */
const settingsParts = ["storage", "compute", "health-checks"] as const
export type SettingsPart = (typeof settingsParts)[number]

/** The parts a pending edit can belong to. */
export type DraftPart = "variables" | "networking" | SettingsPart

/**
 * Names `?tab=` had before: Status, Changes, Configure and Inspect, the tabs before them, and the
 * views that were tabs of their own. The terminal, a view of Logs, is in the dock now.
 */
const formerTabs = [
  "status",
  "changes",
  "configure",
  "inspect",
  "releases",
  "metrics",
  "terminal",
  ...modes,
] as const
type FormerTab = (typeof formerTabs)[number]

/** A place on a service's page: a tab, a view, a part of Settings, or a former tab's name. */
export type ServicePlace = ServiceTab | ServiceMode | SettingsPart | FormerTab

/** Where a place is: the tab, its view, the part of Settings (the hash), and whether the
 * service's terminal opens in the dock. */
export function servicePlace(place?: ServicePlace): {
  tab?: ServiceTab
  mode?: ServiceMode
  hash?: SettingsPart
  terminal?: true
} {
  switch (place) {
    case undefined:
    case "overview":
    case "status":
      return {}
    case "changes":
    case "releases":
      return { tab: "deployments" }
    case "configure":
      return { tab: "variables" }
    case "inspect":
      return { tab: "logs" }
    case "metrics":
      return { tab: "usage" }
    case "terminal":
      return { tab: "logs", terminal: true }
    case "events":
    case "yaml":
      return { tab: serviceModes[place], mode: place }
    case "storage":
    case "compute":
    case "health-checks":
      return { tab: "settings", hash: place }
    default:
      return { tab: place }
  }
}

/**
 * Where a link from before the seven tabs leads: `?tab=inspect&view=…`, `?tab=configure#part`, a
 * former section, a view that was a tab, or a part of Settings named as a tab.
 */
export function formerPlace(
  tab: string,
  view?: string,
  hash?: string,
): ReturnType<typeof servicePlace> {
  if (tab === "inspect" && view === "metrics") return { tab: "usage" }
  if (
    tab === "inspect" &&
    view &&
    [...serviceTabs, ...modes, "terminal"].includes(view as ServiceTab)
  )
    return servicePlace(view as ServicePlace)
  if (tab === "configure" && hash) {
    if (hash === "variables" || hash === "networking") return { tab: hash }
    if ((settingsParts as readonly string[]).includes(hash))
      return { tab: "settings", hash: hash as SettingsPart }
  }
  return servicePlace(tab as ServicePlace)
}

const str = (value: unknown) => (typeof value === "string" && value ? value : undefined)
const oneOf = <T extends string>(values: readonly T[], value: unknown): T | undefined =>
  typeof value === "string" && (values as readonly string[]).includes(value)
    ? (value as T)
    : undefined

export interface ServiceSearch {
  /** Overview when absent. */
  tab?: ServiceTab
  /** The tab's own view when absent. */
  mode?: ServiceMode
  pod?: string
  /** `1h` when absent. */
  range?: Range
  /** Per replica when absent. */
  split?: Split
  /** A `?tab=` from before the seven tabs, kept until the route redirects to its place. */
  former?: string
  /** Inspect's `?view=`, read only to redirect. */
  view?: string
}

export function validateServiceSearch(search: Record<string, unknown>): ServiceSearch {
  const tab = str(search.tab)
  const known = oneOf(serviceTabs, tab)
  const former =
    tab && !known && [...formerTabs, ...settingsParts].includes(tab as FormerTab)
      ? tab
      : search.mode === "terminal"
        ? "terminal"
        : str(search.former)
  const mode = oneOf(modes, search.mode)
  return {
    tab: known === "overview" ? undefined : known,
    mode: mode && known === serviceModes[mode] ? mode : undefined,
    pod: str(search.pod),
    range: oneOf(ranges, search.range),
    split: oneOf(splits, search.split),
    former,
    view: former ? str(search.view) : undefined,
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

const projectSortKeys = ["health", "name"] as const
export type ProjectSortKey = (typeof projectSortKeys)[number]

/** The projects list on Home and Projects; only Projects has the filter. */
export interface ProjectsSearch {
  q?: string
  sort?: ProjectSortKey
  desc?: boolean
}

export function validateProjectsSearch(search: Record<string, unknown>): ProjectsSearch {
  return {
    q: str(search.q),
    sort: oneOf(projectSortKeys, search.sort),
    desc: search.desc === true || search.desc === "true" ? true : undefined,
  }
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
