// The project column: the project's icon and name (a link to its Overview) with a switcher, then its five
// entries (Overview, Architecture, Services, Objects, Settings), its services grouped by `part-of` or by role with a
// status dot each, and the other objects folded away. One highlight with the hinoki rail slides to
// the current entry or service. The column stays in place on every project and service page.
import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { useState, type ReactNode } from "react"
import CaretRightIcon from "~icons/ph/caret-right"
import CubeIcon from "~icons/ph/cube"
import GearSixIcon from "~icons/ph/gear-six"
import GraphIcon from "~icons/ph/graph"
import ListBulletsIcon from "~icons/ph/list-bullets"
import SquaresFourIcon from "~icons/ph/squares-four"

import { projectQuery } from "@/api/queries"
import { ServiceIcon } from "@/components/service-icon"
import { ProjectGlyph } from "@/components/shell/project-glyph"
import { ProjectSwitcher } from "@/components/shell/switchers"
import { StatusDot, StatusPill, useStatusWord } from "@/components/status"
import { Skeleton } from "@/components/ui/skeleton"
import { Tooltip } from "@/components/ui/tooltip"
import { useT } from "@/i18n"
import type { Icon } from "@/lib/icon"
import { addressOf, objectKey } from "@/lib/objects"
import { serviceLink, usePlace, type ProjectEntry } from "@/lib/place"
import { useProjectSummary } from "@/lib/project-name"
import { cn } from "@/lib/utils"
import { projectTitle } from "@/model/project"
import { groupServices, serviceId, statusOf, type Project } from "@/model/service"

/** The Kubernetes objects behind each role, for the group heading's tooltip. */
const roleKube: Record<string, string> = {
  web: "Ingress",
  internal: "Service",
  worker: "Deployment",
  job: "CronJob",
  data: "StatefulSet",
}

const entries: { entry: ProjectEntry; to: string; icon: Icon }[] = [
  { entry: "overview", to: "/projects/$project", icon: SquaresFourIcon },
  { entry: "architecture", to: "/projects/$project/architecture", icon: GraphIcon },
  { entry: "services", to: "/projects/$project/services", icon: ListBulletsIcon },
  { entry: "objects", to: "/projects/$project/objects", icon: CubeIcon },
  { entry: "settings", to: "/projects/$project/settings", icon: GearSixIcon },
]

// Row heights in rem, so the highlight's offset follows the rows at any root font size.
const ITEM = 2
const HEAD = 1.875
const DIVIDER = 1.0625

const itemClass =
  "flex h-8 w-full items-center gap-2.5 rounded-md pr-3 pl-3.5 text-left text-muted-foreground outline-none transition-colors hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring focus-visible:ring-inset"
const currentClass = "font-medium text-foreground"

export function ProjectColumn({ project, className }: { project: string; className?: string }) {
  const t = useT()
  const view = useQuery(projectQuery(project))
  const data = view.data
  const summary = useProjectSummary(project)

  return (
    <aside
      aria-label={t("column.label")}
      className={cn("flex h-full w-60 shrink-0 flex-col border-r bg-sidebar text-sm", className)}
    >
      <div className="flex h-14 shrink-0 items-center gap-2 border-b pr-3 pl-4">
        <ProjectGlyph icon={summary?.icon} />
        <Link
          to="/projects/$project"
          params={{ project }}
          className="min-w-0 flex-1 truncate rounded-md type-heading font-semibold outline-none hover:underline focus-visible:ring-3 focus-visible:ring-ring"
          translate="no"
          title={summary?.displayName ? project : undefined}
        >
          {projectTitle(summary, project)}
        </Link>
        <ProjectSwitcher current={project} trigger="updown" />
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto">
        <ColumnNav project={project} data={data} />
      </div>
    </aside>
  )
}

function ColumnNav({ project, data }: { project: string; data?: Project }) {
  const t = useT()
  const place = usePlace()
  const word = useStatusWord()
  const [otherOpen, setOtherOpen] = useState(false)

  // Build the rows top to bottom, keeping the current item's offset for the highlight.
  const rows: ReactNode[] = []
  let y = 0
  let highlight: number | undefined

  for (const { entry, to, icon: EntryIcon } of entries) {
    const current = place.entry === entry
    if (current) highlight = y
    y += ITEM
    rows.push(
      <Link
        key={entry}
        to={to}
        params={{ project }}
        aria-current={current ? "page" : undefined}
        className={cn(itemClass, current && currentClass)}
      >
        <EntryIcon className="size-4 shrink-0" aria-hidden="true" />
        <span className="flex-1 truncate">{t(`project.entry.${entry}`)}</span>
        {entry === "services" && data ? (
          <span className="type-meta text-muted-foreground">{data.services.length}</span>
        ) : null}
      </Link>,
    )
  }

  if (data) {
    rows.push(<Divider key="d1" />)
    y += DIVIDER
    for (const group of groupServices(data.services)) {
      y += HEAD
      rows.push(
        <div
          key={`h-${group.key}`}
          className="flex h-7.5 items-end justify-between pr-3 pb-1.5 pl-3.5 type-meta font-medium text-muted-foreground"
        >
          {group.role ? (
            <Tooltip content={roleKube[group.role]}>
              <span>{t(`role.${group.role}`)}</span>
            </Tooltip>
          ) : (
            <span translate="no">{group.name}</span>
          )}
          <span className="font-normal">{group.services.length}</span>
        </div>,
      )
      for (const service of group.services) {
        const id = serviceId(service)
        const current = !place.entry && place.service === id
        const status = statusOf(service)
        if (current) highlight = y
        y += ITEM
        rows.push(
          <Link
            key={id}
            {...serviceLink(project, service, { tab: place.entry ? undefined : place.tab })}
            aria-current={current ? "page" : undefined}
            className={cn(itemClass, current && currentClass)}
          >
            <ServiceIcon service={service} className="text-current" />
            <span className="min-w-0 flex-1 truncate" translate="no">
              {service.displayName ?? service.name}
            </span>
            <Tooltip content={word(status)}>
              <span className="flex size-4 items-center justify-center">
                <StatusDot health={status.health} />
                <span className="sr-only">{word(status)}</span>
              </span>
            </Tooltip>
          </Link>,
        )
      }
    }
    if (data.resources.length > 0) {
      const unused = data.resources.filter((r) => r.attribution.ownership === "orphan").length
      rows.push(<Divider key="d2" />)
      rows.push(
        <button
          key="other"
          type="button"
          onClick={() => setOtherOpen(!otherOpen)}
          aria-expanded={otherOpen}
          className={itemClass}
        >
          <CaretRightIcon
            className={cn("size-4 shrink-0 transition-transform", otherOpen && "rotate-90")}
            aria-hidden="true"
          />
          <span className="flex-1 truncate">{t("column.other")}</span>
          {unused > 0 ? (
            <StatusPill health="warn" size="sm" dot={false}>
              {t("resource.unusedCount", unused)}
            </StatusPill>
          ) : (
            <span className="type-meta">{data.resources.length}</span>
          )}
        </button>,
      )
      if (otherOpen)
        rows.push(
          <ul key="resources" className="flex flex-col pb-2">
            {data.resources.map((r) => (
              <li key={`${r.kind}/${r.name}`}>
                <Link
                  to="/projects/$project/objects"
                  params={{ project }}
                  search={{ object: objectKeyOf(r.kind, r.name) }}
                  className="flex h-7 items-center gap-2 rounded-md pr-3 pl-9.5 type-meta text-muted-foreground outline-none hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring focus-visible:ring-inset"
                >
                  <span className="min-w-0 truncate" translate="no">
                    {r.name}
                  </span>
                  <span className="shrink-0 opacity-70">{r.kind}</span>
                  {r.attribution.ownership === "orphan" ? (
                    <StatusPill health="warn" size="sm" dot={false} className="ml-auto">
                      {t("resource.unused")}
                    </StatusPill>
                  ) : null}
                </Link>
              </li>
            ))}
          </ul>,
        )
    }
  }

  return (
    <nav aria-label={t("project.sections")} className="relative p-2 pb-4">
      <div
        aria-hidden="true"
        className={cn(
          "slide pointer-events-none absolute inset-x-2 top-2 h-8 translate-y-(--slide-y) rounded-md bg-muted",
          highlight === undefined && "opacity-0",
        )}
        style={{ "--slide-y": `${highlight ?? 0}rem` }}
      >
        <span className="absolute top-1.75 bottom-1.75 left-0 w-0.75 rounded-full bg-brand" />
      </div>
      <div className="relative flex flex-col">{rows}</div>
      {data ? null : (
        <div className="flex flex-col gap-1.5 pt-4">
          {Array.from({ length: 5 }, (_, i) => (
            <Skeleton key={i} className="h-7 w-full" />
          ))}
        </div>
      )}
    </nav>
  )
}

/** `?object=` for a resource the projection names. */
function objectKeyOf(kind: string, name: string): string | undefined {
  const address = addressOf(kind, name)
  return address ? objectKey(address) : undefined
}

function Divider() {
  return (
    <div aria-hidden="true" className="flex h-4.25 items-center px-3">
      <span className="h-px w-full bg-border" />
    </div>
  )
}
