// The top of a project entry. On wide screens the entries live at the top of the project column,
// so the header is the entry's title only (the Overview draws its own, with the project's health).
// On phones, where the column sits behind the menu, the everyday entries also show here as tabs;
// Objects (folded under Advanced in the column) only while it is the page shown.
import { Link } from "@tanstack/react-router"

import { useT } from "@/i18n"
import type { ProjectEntry } from "@/lib/place"
import { cn } from "@/lib/utils"
import { PageHeader } from "@engawahq/design-system/components/page-header"

const everyday: { entry: ProjectEntry; to: string }[] = [
  { entry: "overview", to: "/projects/$project" },
  { entry: "services", to: "/projects/$project/services" },
  { entry: "architecture", to: "/projects/$project/architecture" },
  { entry: "settings", to: "/projects/$project/settings" },
]
const objects = { entry: "objects", to: "/projects/$project/objects" } as const

export function ProjectHeader({
  project,
  entry,
  className,
}: {
  project: string
  entry: ProjectEntry
  className?: string
}) {
  const t = useT()
  return (
    <header
      className={cn("flex shrink-0 flex-col", entry === "architecture" && "border-b", className)}
    >
      <nav
        aria-label={t("project.sections")}
        className="flex gap-1 overflow-x-auto border-b px-4 scrollbar-none md:hidden"
      >
        {(entry === "objects" ? [...everyday, objects] : everyday).map(({ entry: value, to }) => {
          const active = value === entry
          return (
            <Link
              key={value}
              to={to}
              params={{ project }}
              activeOptions={{ exact: true, includeSearch: false }}
              aria-current={active ? "page" : undefined}
              className={cn(
                "-mb-px flex h-10 shrink-0 items-center border-b-2 px-2.5 text-sm outline-none",
                active
                  ? "border-brand font-medium text-foreground"
                  : "border-transparent text-muted-foreground hover:text-foreground",
              )}
            >
              {t(`project.entry.${value}`)}
            </Link>
          )
        })}
      </nav>
      {entry === "overview" ? null : (
        <div className="px-4 pt-6 pb-4 md:px-8">
          <PageHeader title={t(`project.entry.${entry}`)} />
        </div>
      )}
    </header>
  )
}
