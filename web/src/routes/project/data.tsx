// Loads one project and its events for the project pages, with one loading and one error state.
import { useQuery } from "@tanstack/react-query"
import type { ReactNode } from "react"

import type { Schemas } from "@/api/client"
import { eventsQuery, projectQuery } from "@/api/queries"
import { LoadingRows } from "@/components/page"
import { Problem } from "@/components/problem"
import { useT } from "@/i18n"
import type { Event, Project } from "@/model/service"
import { PageColumn } from "@zeabur/design-system/components/page-column"

const noEvents: Event[] = []

export function useProjectEvents(project: string): Event[] {
  return useQuery(eventsQuery(project)).data?.items ?? noEvents
}

export function ProjectData({
  project,
  children,
}: {
  project: string
  children: (data: Project, unreadable: Schemas["UnreadableView"][]) => ReactNode
}) {
  const t = useT()
  const view = useQuery(projectQuery(project))
  if (view.isPending)
    return (
      <PageColumn className="my-6">
        <LoadingRows label={t("common.loading")} rows={6} />
      </PageColumn>
    )
  // A failed refresh keeps what was read; the header's freshness says it failed.
  if (!view.data)
    return (
      <PageColumn className="my-6">
        <Problem error={view.error} />
      </PageColumn>
    )
  return children(view.data, view.data.unreadable)
}

/** Where the project's pages show a kind the person may not list: the section its objects feed. */
export const refusedPlaces = {
  /** The services and their pods. */
  services: [
    "deployments.apps",
    "statefulsets.apps",
    "daemonsets.apps",
    "jobs.batch",
    "cronjobs.batch",
    "pods",
  ],
  events: ["events.events.k8s.io"],
  /** Domains: the objects that route to a service. */
  routes: ["ingresses.networking.k8s.io", "httproutes.gateway.networking.k8s.io"],
  /** Kubernetes Services: a service's private names. */
  endpoints: ["services"],
  policies: ["networkpolicies.networking.k8s.io"],
  /** Where variables come from. */
  config: ["secrets", "configmaps"],
  claims: ["persistentvolumeclaims"],
} as const

/** The kinds a tab of the service page shows in place: Variables, Networking, Storage, Events.
 * The rest (pods, workloads) share one line under the service's header. */
export const inServiceTabs: readonly string[] = [
  ...refusedPlaces.config,
  ...refusedPlaces.routes,
  ...refusedPlaces.endpoints,
  ...refusedPlaces.policies,
  ...refusedPlaces.claims,
  ...refusedPlaces.events,
]
