import { useQuery } from "@tanstack/react-query"

import { projectQuery } from "@/api/queries"

/** Names a new service may not take: those of the project's services, endpoints and routes. */
export function useTakenNames(project: string): string[] {
  const view = useQuery(projectQuery(project)).data
  return view
    ? [
        ...view.services.map((s) => s.name),
        ...view.endpoints.map((e) => e.name),
        ...view.routes.map((r) => r.name),
      ]
    : []
}
