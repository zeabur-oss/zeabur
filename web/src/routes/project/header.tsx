import type { ReactNode } from "react"

import { useT } from "@/i18n"
import type { ProjectEntry } from "@/lib/place"
import { PageColumn } from "@zeabur/design-system/components/page-column"
import { PageHeader } from "@zeabur/design-system/components/page-header"

export function ProjectHeader({
  entry,
  description,
  actions,
}: {
  entry: Exclude<ProjectEntry, "overview">
  description?: ReactNode
  actions?: ReactNode
}) {
  const t = useT()
  return (
    <header className="shrink-0 pt-6 pb-4 md:pt-8">
      <PageColumn>
        <PageHeader
          title={t(`project.entry.${entry}`)}
          description={description}
          actions={actions}
        />
      </PageColumn>
    </header>
  )
}
