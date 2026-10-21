// Where the person is (the open project, entry, service and section) for the shell, and the links
// every page uses to reach a project entry or a place on a service.
import { useMatches, type RegisteredRouter, type RouteIds } from "@tanstack/react-router"

import { servicePlace, type ServicePlace } from "@/routes/search"

export type ProjectEntry = "overview" | "objects" | "terminal" | "settings"

type RouteId = RouteIds<RegisteredRouter["routeTree"]>

export interface Place {
  /** The route the page belongs to (`__root__` when no route matched). */
  route: RouteId
  project?: string
  entry?: ProjectEntry
  /** `Kind/name` of the service on the service page, or in the project's quick look. */
  service?: string
  /** The tab, or the tab's view, that a link to another service keeps. */
  tab?: ServicePlace
}

const entries: Partial<Record<RouteId, ProjectEntry>> = {
  "/projects/$project": "overview",
  "/projects/$project/objects": "objects",
  "/projects/$project/settings": "settings",
  "/projects/$project/terminal": "terminal",
}

/** The place, from the route the router matched (the deepest match). */
export function usePlace(): Place {
  return useMatches({
    select: (matches): Place => {
      const match = matches.at(-1)
      if (!match) return { route: "__root__" }
      const route = match.routeId
      if (match.routeId === "/projects/$project/services/$kind/$name") {
        const { project, kind, name } = match.params
        const { tab, mode } = match.search
        return { route, project, service: `${kind}/${name}`, tab: mode ?? tab }
      }
      const entry = entries[route]
      if (!entry || !("project" in match.params)) return { route }
      return {
        route,
        project: match.params.project,
        entry,
        service: match.routeId === "/projects/$project" ? match.search.service : undefined,
      }
    },
    structuralSharing: true,
  })
}

export function serviceLink(
  project: string,
  service: { kind: string; name: string },
  { tab, pod }: { tab?: ServicePlace; pod?: string } = {},
) {
  const place = servicePlace(tab)
  return {
    to: "/projects/$project/services/$kind/$name" as const,
    params: { project, kind: service.kind, name: service.name },
    search: { tab: place.tab, mode: place.mode, pod },
    hash: place.hash,
  }
}

/** The service's logs with one replica chosen. */
export const podLogsLink = (
  project: string,
  service: { kind: string; name: string },
  pod: string,
) => serviceLink(project, service, { tab: "logs", pod })

export const podEventsLink = (
  project: string,
  service: { kind: string; name: string },
  pod: string,
) => serviceLink(project, service, { tab: "events", pod })
