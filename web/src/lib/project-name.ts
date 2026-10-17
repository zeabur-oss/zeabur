// The project list's entry for one namespace, which carries its display name and icon: what the
// shell names a project by, without reading its project document.
import { useQuery } from "@tanstack/react-query"

import type { Schemas } from "@/api/client"
import { projectsQuery } from "@/api/queries"

export function useProjectSummary(name: string): Schemas["ProjectSummary"] | undefined {
  return useQuery({
    ...projectsQuery,
    select: (list) => list.items.find((p) => p.name === name),
  }).data
}
