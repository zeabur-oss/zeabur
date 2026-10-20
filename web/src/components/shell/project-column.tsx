// The project column, a pane at the left of the content panel (the design system's sidebar parts):
// the project's icon and name (a link to its Overview) with a switcher, then its entries
// (Overview, Services, Architecture and Settings; Objects folded under Advanced), its services
// grouped by `part-of` or by role with a status dot each, and the other objects folded away. The
// current entry or service has the selected fill: the one current row in the frame (the global sidebar
// then has none). The column stays in place on every project and service page.
import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
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
import { useT } from "@/i18n"
import type { Icon } from "@/lib/icon"
import { addressOf, objectKey } from "@/lib/objects"
import { serviceLink, usePlace, type ProjectEntry } from "@/lib/place"
import { remembered } from "@/lib/stores"
import { useProjectSummary } from "@/lib/project-name"
import { cn } from "@/lib/utils"
import { projectTitle } from "@/model/project"
import { groupServices, serviceId, statusOf, type Project } from "@/model/service"
import {
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarItem,
  SidebarSection,
} from "@zeabur/design-system/components/sidebar"
import { Skeleton } from "@zeabur/design-system/components/skeleton"
import { Tooltip } from "@zeabur/design-system/components/tooltip"

/** The Kubernetes objects behind each role, for the group heading's tooltip. */
const roleKube: Record<string, string> = {
  web: "Ingress",
  internal: "Service",
  worker: "Deployment",
  job: "CronJob",
  data: "StatefulSet",
}

type Entry = { entry: ProjectEntry; to: string; icon: Icon }

// The everyday path (web/DESIGN.md, "Depth"), then what takes knowing Kubernetes, folded.
const everyday: Entry[] = [
  { entry: "overview", to: "/projects/$project", icon: SquaresFourIcon },
  { entry: "services", to: "/projects/$project/services", icon: ListBulletsIcon },
  { entry: "architecture", to: "/projects/$project/architecture", icon: GraphIcon },
  { entry: "settings", to: "/projects/$project/settings", icon: GearSixIcon },
]
const advanced: Entry[] = [{ entry: "objects", to: "/projects/$project/objects", icon: CubeIcon }]

const advancedSection = remembered("zeabur.column.advanced", false)

export function ProjectColumn({ project, className }: { project: string; className?: string }) {
  const t = useT()
  const view = useQuery(projectQuery(project))
  const data = view.data
  const summary = useProjectSummary(project)

  return (
    <aside
      aria-label={t("column.label")}
      className={cn("flex h-full w-60 shrink-0 flex-col border-r", className)}
    >
      <div className="flex h-11 shrink-0 items-center gap-2 border-b pr-2 pl-4">
        <ProjectGlyph icon={summary?.icon} />
        <Link
          to="/projects/$project"
          params={{ project }}
          activeOptions={{ exact: true, includeSearch: false }}
          className="min-w-0 flex-1 truncate rounded-md font-semibold outline-none hover:underline"
          translate="no"
          title={summary?.displayName ? project : undefined}
        >
          {projectTitle(summary, project)}
        </Link>
        <ProjectSwitcher current={project} trigger="updown" />
      </div>
      <ColumnNav project={project} data={data} />
    </aside>
  )
}

function ColumnNav({ project, data }: { project: string; data?: Project }) {
  const t = useT()
  const place = usePlace()
  const word = useStatusWord()
  const advancedOpen = advancedSection.useValue()
  const advancedHolds = advanced.some((e) => e.entry === place.entry)
  const entryRow = ({ entry, to, icon: EntryIcon }: Entry) => (
    <SidebarItem
      key={entry}
      render={
        <Link to={to} params={{ project }} activeOptions={{ exact: true, includeSearch: false }} />
      }
      icon={<EntryIcon aria-hidden="true" />}
      current={place.entry === entry}
      // A service is open: it is the current row, under Services.
      ancestor={entry === "services" && !place.entry && !!place.service}
      trailing={entry === "services" && data ? data.services.length : undefined}
    >
      {t(`project.entry.${entry}`)}
    </SidebarItem>
  )

  return (
    <Sidebar aria-label={t("project.sections")} className="min-h-0 flex-1">
      <SidebarContent>
        <SidebarGroup>{everyday.map(entryRow)}</SidebarGroup>
        {data ? (
          <>
            {groupServices(data.services).map((group) => (
              <SidebarSection
                key={group.key}
                title={
                  group.role ? (
                    <Tooltip content={roleKube[group.role]}>
                      <span>{t(`role.${group.role}`)}</span>
                    </Tooltip>
                  ) : (
                    <span translate="no">{group.name}</span>
                  )
                }
                action={
                  <span className="pr-1 type-meta text-muted-foreground">
                    {group.services.length}
                  </span>
                }
              >
                {group.services.map((service) => {
                  const id = serviceId(service)
                  const status = statusOf(service)
                  return (
                    <SidebarItem
                      key={id}
                      render={
                        <Link
                          {...serviceLink(project, service, {
                            tab: place.entry ? undefined : place.tab,
                          })}
                          activeOptions={{ exact: true, includeSearch: false }}
                        />
                      }
                      icon={<ServiceIcon service={service} className="text-current" />}
                      current={!place.entry && place.service === id}
                      trailing={
                        <Tooltip content={word(status)}>
                          <span className="flex size-4 items-center justify-center">
                            <StatusDot health={status.health} />
                            <span className="sr-only">{word(status)}</span>
                          </span>
                        </Tooltip>
                      }
                    >
                      <span translate="no">{service.displayName ?? service.name}</span>
                    </SidebarItem>
                  )
                })}
              </SidebarSection>
            ))}
            {data.resources.length > 0 ? <OtherObjects project={project} data={data} /> : null}
          </>
        ) : (
          <div className="flex flex-col gap-1.5 pt-4">
            {Array.from({ length: 5 }, (_, i) => (
              <Skeleton key={i} className="h-7 w-full" />
            ))}
          </div>
        )}
        <SidebarSection
          title={t("column.advanced")}
          // Open while it holds the page shown: the current row is never folded away.
          open={advancedOpen || advancedHolds}
          // Held open by the page shown, a click is not a choice to remember.
          onOpenChange={(open) => {
            if (!advancedHolds) advancedSection.set(open)
          }}
        >
          {advanced.map(entryRow)}
        </SidebarSection>
      </SidebarContent>
    </Sidebar>
  )
}

/** The objects that belong to no service, folded away; the unused ones are counted. */
function OtherObjects({ project, data }: { project: string; data: Project }) {
  const t = useT()
  const unused = data.resources.filter((r) => r.attribution.ownership === "orphan").length
  return (
    <SidebarSection
      title={t("column.other")}
      defaultOpen={false}
      action={
        unused > 0 ? (
          <StatusPill health="warn" size="sm" dot={false}>
            {t("resource.unusedCount", unused)}
          </StatusPill>
        ) : (
          <span className="pr-1 type-meta text-muted-foreground">{data.resources.length}</span>
        )
      }
    >
      {data.resources.map((r) => (
        <SidebarItem
          key={`${r.kind}/${r.name}`}
          tone="child"
          render={
            <Link
              to="/projects/$project/objects"
              params={{ project }}
              search={{ object: objectKeyOf(r.kind, r.name) }}
              // These rows share one path and differ by `?object=`: match the search, and only
              // as far as it goes (the page may also have `?q`). Nothing lives under /objects.
              activeOptions={{ exact: false }}
            />
          }
          trailing={
            r.attribution.ownership === "orphan" ? (
              <StatusPill health="warn" size="sm" dot={false}>
                {t("resource.unused")}
              </StatusPill>
            ) : (
              <span className="opacity-70">{r.kind}</span>
            )
          }
        >
          <span translate="no">{r.name}</span>
        </SidebarItem>
      ))}
    </SidebarSection>
  )
}

/** `?object=` for a resource the projection names. */
function objectKeyOf(kind: string, name: string): string | undefined {
  const address = addressOf(kind, name)
  return address ? objectKey(address) : undefined
}
