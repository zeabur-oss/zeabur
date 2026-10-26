import * as z from "zod/mini"

import { ranges, splits, type Range, type Split } from "@/model/usage"

/** A service's tabs, in their order. */
export const serviceTabs = [
  "overview",
  "logs",
  "deployments",
  "builds",
  "usage",
  "networking",
  "variables",
  "files",
  "settings",
] as const
export type ServiceTab = (typeof serviceTabs)[number]

/** Views for people who know Kubernetes, each a second view of a tab, in `?mode=`. */
export const serviceModes = { events: "deployments", yaml: "settings" } as const
export type ServiceMode = keyof typeof serviceModes
const modes = Object.keys(serviceModes) as ServiceMode[]

/** The parts of Settings, each an anchor. */
const settingsParts = [
  "build",
  "schedule",
  "storage",
  "compute",
  "health-checks",
  "security",
  "tasks",
] as const
export type SettingsPart = (typeof settingsParts)[number]

/** The parts a pending edit can belong to. */
export type DraftPart = "variables" | "networking" | SettingsPart

/** A place on a service's page: a tab, a view, or a part of Settings. */
export type ServicePlace = ServiceTab | ServiceMode | SettingsPart

/** Where a place is: the tab, its view, and the part of Settings (the hash). */
export function servicePlace(place?: ServicePlace): {
  tab?: ServiceTab
  mode?: ServiceMode
  hash?: SettingsPart
} {
  switch (place) {
    case undefined:
    case "overview":
      return {}
    case "events":
    case "yaml":
      return { tab: serviceModes[place], mode: place }
    case "build":
    case "schedule":
    case "storage":
    case "compute":
    case "health-checks":
    case "security":
    case "tasks":
      return { tab: "settings", hash: place }
    default:
      return { tab: place }
  }
}

// An unknown or empty value reads as absent. No search param makes a page fail.
const text = z.catch(z.optional(z.string().check(z.minLength(1))), undefined)
const choice = <const T extends readonly string[]>(values: T) =>
  z.catch(z.optional(z.enum(values)), undefined)
const flag = z.catch(
  z.optional(
    z.pipe(
      z.union([z.boolean(), z.literal("true")]),
      z.transform((value) => (value === false ? undefined : true)),
    ),
  ),
  undefined,
)

interface ServiceSearch {
  /** Overview when absent. */
  tab?: ServiceTab
  /** The tab's own view when absent. */
  mode?: ServiceMode
  pod?: string
  /** `1h` when absent. */
  range?: Range
  /** Per replica when absent. */
  split?: Split
}

export const serviceSearch = z.pipe(
  z.object({
    tab: choice(serviceTabs),
    mode: choice(modes),
    pod: text,
    range: choice(ranges),
    split: choice(splits),
  }),
  z.transform(({ tab, mode, pod, range, split }): ServiceSearch => ({
    tab: tab === "overview" ? undefined : tab,
    mode: mode && tab === serviceModes[mode] ? mode : undefined,
    pod,
    range,
    split,
  })),
)

/** A database's tabs, in their order. */
export const databaseTabs = ["overview", "logs", "backups", "events", "settings"] as const
export type DatabaseTab = (typeof databaseTabs)[number]

/** The search of a database's or an object store's page. */
interface StoreSearch<Tab> {
  /** Overview when absent. */
  tab?: Tab
  /** Settings' YAML view; only with Settings. */
  mode?: "yaml"
  pod?: string
}

const storeSearch = <const Tabs extends readonly string[]>(tabs: Tabs) =>
  z.pipe(
    z.object({ tab: choice(tabs), mode: choice(["yaml"]), pod: text }),
    z.transform(({ tab, mode, pod }): StoreSearch<Tabs[number]> => ({
      tab: tab === "overview" ? undefined : tab,
      mode: tab === "settings" ? mode : undefined,
      pod: tab === "logs" || tab === "events" ? pod : undefined,
    })),
  )

export const databaseSearch = storeSearch(databaseTabs)

export const objectStorageTabs = ["overview", "logs", "events", "settings"] as const
export type ObjectStorageTab = (typeof objectStorageTabs)[number]

export const objectStorageSearch = storeSearch(objectStorageTabs)

export const objectsSearch = z.object({
  q: text,
  /** `group/version/resource/name` of the object whose YAML is open. */
  object: text,
  generated: flag,
  empty: flag,
})

export const profiles = ["single-node", "multi-node"] as const

export const componentsSearch = z.object({
  /** `capability/id` of the component whose install texts are open. */
  component: text,
  /** The cluster's own when absent. */
  profile: choice(profiles),
})

/** What GitHub sends back at the end of its manifest flow. */
export const gitHubAppSearch = z.object({
  code: text,
  state: text,
})

const projectSortKeys = ["health", "name"] as const
export type ProjectSortKey = (typeof projectSortKeys)[number]

/** The projects list on Home and Projects; only Projects has the filter. */
export const projectsSearch = z.object({
  q: text,
  sort: choice(projectSortKeys),
  desc: flag,
})
export type ProjectsSearch = z.output<typeof projectsSearch>

const statusFilters = ["attention", "healthy"] as const
const sortKeys = ["name", "status", "ready", "restarts", "kind"] as const
export type SortKey = (typeof sortKeys)[number]
const projectViews = ["map", "list"] as const
export type ProjectView = (typeof projectViews)[number]

export const projectSearch = z.object({
  /** The one last chosen when absent; a list for a project too big to draw. */
  view: choice(projectViews),
  /** `Kind/name` of the service in the quick look. */
  service: text,
  /** Absent: warnings only. */
  events: choice(["all"]),
  q: text,
  status: choice(statusFilters),
  kind: text,
  sort: choice(sortKeys),
  desc: flag,
})
export type ProjectSearch = z.output<typeof projectSearch>

/** The search of a page with a service's tabs once `next` is open: the pod stays on Logs and
 * Events. */
export const tabSearch =
  <Tab extends string>(next: Tab) =>
  (previous: { pod?: string }) => ({
    tab: next === "overview" ? undefined : next,
    pod: next === "logs" || next === "events" ? previous.pod : undefined,
  })

export const modeSearch =
  (mode?: ServiceMode) =>
  <S extends object>(previous: S) => ({
    ...previous,
    mode: mode === "yaml" ? ("yaml" as const) : undefined,
  })

export const podSearch =
  (pod?: string) =>
  <S extends { pod?: string }>(previous: S) => ({ ...previous, pod: pod || undefined })
