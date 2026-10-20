import { useT } from "@/i18n"
import type { ProjectEntry } from "@/lib/place"
import { cn } from "@/lib/utils"
import { PageHeader } from "@zeabur/design-system/components/page-header"

export type Column = "content" | "wide" | "full"

export function ProjectHeader({
  entry,
  column = "content",
}: {
  entry: Exclude<ProjectEntry, "overview">
  column?: Column
}) {
  const t = useT()
  return (
    <header
      className={cn(
        "shrink-0 px-4 pt-6 pb-4 md:px-8 md:pt-8",
        column === "full" && "border-b border-main pb-5",
      )}
    >
      <div
        className={cn(
          "w-full",
          column === "content" && "mx-auto max-w-content",
          column === "wide" && "mx-auto max-w-6xl",
        )}
      >
        <PageHeader title={t(`project.entry.${entry}`)} />
      </div>
    </header>
  )
}
