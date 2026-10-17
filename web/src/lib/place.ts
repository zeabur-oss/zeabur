// Where the person is (the open project, entry, service and tab) for the shell, and the links
// every page uses to reach a project entry or a service.
import { useMatches, type RegisteredRouter, type RouteIds } from "@tanstack/react-router"

import type { ServiceTab } from "@/routes/search"

export type ProjectEntry = "overview" | "architecture" | "services" | "objects" | "settings"

type RouteId = RouteIds<RegisteredRouter["routeTree"]>

export interface Place {
  /** The route the page belongs to (`__root__` when no route matched). */
  route: RouteId
  project?: string
  entry?: ProjectEntry
  /** `Kind/name` of the service on the service page, or in the Architecture quick look. */
  service?: string
  tab?: ServiceTab
}

const entries: Partial<Record<RouteId, ProjectEntry>> = {
  "/projects/$project": "overview",
  "/projects/$project/architecture": "architecture",
  "/projects/$project/services": "services",
  "/projects/$project/objects": "objects",
  "/projects/$project/settings": "settings",
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
        return { route, project, service: `${kind}/${name}`, tab: match.search.tab }
      }
      const entry = entries[route]
      if (!entry || !("project" in match.params)) return { route }
      return {
        route,
        project: match.params.project,
        entry,
        service:
          match.routeId === "/projects/$project/architecture" ? match.search.service : undefined,
      }
    },
    structuralSharing: true,
  })
}

export function serviceLink(
  project: string,
  service: { kind: string; name: string },
  search: { tab?: ServiceTab; pod?: string } = {},
) {
  return {
    to: "/projects/$project/services/$kind/$name" as const,
    params: { project, kind: service.kind, name: service.name },
    search: { tab: search.tab, pod: search.pod },
  }
}

/** The service's Logs tab with one pod chosen. */
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
