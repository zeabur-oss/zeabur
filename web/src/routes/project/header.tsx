// The top of a project entry. On wide screens the five entries live at the top of the project
// column, so the header is the entry's title only (the Overview draws its own, with the project's
// health). On phones, where the column sits behind the menu, the entries also show here as tabs.
import { Link } from "@tanstack/react-router"

import { useT } from "@/i18n"
import type { ProjectEntry } from "@/lib/place"
import { cn } from "@/lib/utils"

const entries: { entry: ProjectEntry; to: string }[] = [
  { entry: "overview", to: "/projects/$project" },
  { entry: "architecture", to: "/projects/$project/architecture" },
  { entry: "services", to: "/projects/$project/services" },
  { entry: "objects", to: "/projects/$project/objects" },
  { entry: "settings", to: "/projects/$project/settings" },
]

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
        {entries.map(({ entry: value, to }) => {
          const active = value === entry
          return (
            <Link
              key={value}
              to={to}
              params={{ project }}
              aria-current={active ? "page" : undefined}
              className={cn(
                "-mb-px flex h-10 shrink-0 items-center border-b-2 px-2.5 text-sm outline-none focus-visible:ring-3 focus-visible:ring-ring",
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
        <h1 className="px-4 pt-6 pb-4 type-title md:px-8">{t(`project.entry.${entry}`)}</h1>
      )}
    </header>
  )
}
