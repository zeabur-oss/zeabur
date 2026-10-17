// A project's icon in the shell (sidebar, switcher, project column). The icon set loads in its own
// chunk after the first page, so the first load carries none of it (scripts/ui-budget.sh); until
// then an empty box of the same size holds the place.
import { lazy, Suspense } from "react"

import { cn } from "@/lib/utils"
import type { ProjectIconName } from "@/model/project"

const ProjectIcon = lazy(() =>
  import("@/components/project-icon").then((m) => ({ default: m.ProjectIcon })),
)

export function ProjectGlyph({
  icon,
  className,
}: {
  icon?: ProjectIconName | null
  className?: string
}) {
  return (
    <Suspense fallback={<span aria-hidden="true" className={cn("size-4 shrink-0", className)} />}>
      <ProjectIcon icon={icon} className={className} />
    </Suspense>
  )
}
