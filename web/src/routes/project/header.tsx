import { useT } from "@/i18n"
import type { ProjectEntry } from "@/lib/place"
import { PageColumn } from "@zeabur/design-system/components/page-column"
import { PageHeader } from "@zeabur/design-system/components/page-header"

export function ProjectHeader({ entry }: { entry: Exclude<ProjectEntry, "overview"> }) {
  const t = useT()
  return (
    <header className="shrink-0 pt-6 pb-4 md:pt-8">
      <PageColumn>
        <PageHeader title={t(`project.entry.${entry}`)} />
      </PageColumn>
    </header>
  )
}
