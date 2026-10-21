import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import ArrowLeftIcon from "~icons/ph/arrow-left"
import CubeIcon from "~icons/ph/cube"
import FolderSimpleIcon from "~icons/ph/folder-simple"
import GearSixIcon from "~icons/ph/gear-six"
import InfoIcon from "~icons/ph/info"
import SquaresFourIcon from "~icons/ph/squares-four"
import StackIcon from "~icons/ph/stack"

import { projectQuery } from "@/api/queries"
import { RoleIcon, ServiceIcon } from "@/components/service-icon"
import { DockToggle } from "@/components/shell/dock-slot"
import { ContextMenu } from "@/components/shell/user-menu"
import { ProjectGlyph } from "@/components/shell/project-glyph"
import { SearchRow } from "@/components/shell/search-row"
import { ProjectSwitcher } from "@/components/shell/switchers"
import { StatusDot, StatusPill, useStatusWord } from "@/components/status"
import { useT } from "@/i18n"
import type { Icon } from "@/lib/icon"
import { addressOf, objectKey } from "@/lib/objects"
import { serviceLink, usePlace, type ProjectEntry } from "@/lib/place"
import { useProjectSummary } from "@/lib/project-name"
import { remembered } from "@/lib/stores"
import { projectTitle } from "@/model/project"
import { groupServices, serviceId, statusOf, type Project, type Service } from "@/model/service"
import {
  Sidebar,
  SidebarContent,
  SidebarCopyPill,
  SidebarFooter,
  SidebarGroup,
  SidebarItem,
  SidebarMenu,
  SidebarSection,
} from "@zeabur/design-system/components/sidebar"
import { Skeleton } from "@zeabur/design-system/components/skeleton"
import { Tooltip } from "@zeabur/design-system/components/tooltip"

const roleKube: Record<string, string> = {
  web: "Ingress",
  internal: "Service",
  worker: "Deployment",
  job: "CronJob",
  data: "StatefulSet",
}

type Entry = { entry: ProjectEntry; to: string; icon: Icon }

const pages: Entry[] = [
  { entry: "overview", to: "/projects/$project", icon: SquaresFourIcon },
  { entry: "settings", to: "/projects/$project/settings", icon: GearSixIcon },
]

const advancedSection = remembered("zeabur.column.advanced", false)

export function ProjectSidebar({ project }: { project: string }) {
  const place = usePlace()
  if (place.entry === "settings") return <SettingsSidebar project={project} />
  return <ProjectNav project={project} />
}

function SettingsSidebar({ project }: { project: string }) {
  const t = useT()
  return (
    <Sidebar aria-label={t("projectSettings.title")}>
      <SidebarItem
        render={
          <Link
            to="/projects/$project"
            params={{ project }}
            activeOptions={{ exact: true, includeSearch: false }}
          />
        }
        icon={<ArrowLeftIcon aria-hidden="true" />}
      >
        {t("projectSettings.back")}
      </SidebarItem>
      <h2 className="shrink-0 truncate px-2 type-name">{t("projectSettings.title")}</h2>
      <SidebarContent>
        <SidebarGroup>
          <SidebarItem
            render={
              <Link
                to="/projects/$project/settings"
                params={{ project }}
                activeOptions={{ exact: true, includeSearch: false }}
              />
            }
            icon={<InfoIcon aria-hidden="true" />}
            current
          >
            {t("projectSettings.general")}
          </SidebarItem>
        </SidebarGroup>
      </SidebarContent>
      <SidebarCopyPill label={t("column.namespace")} value={project} copyLabel={t("common.copy")} />
      <SidebarFooter>
        <DockToggle />
        <ContextMenu />
      </SidebarFooter>
    </Sidebar>
  )
}

function ProjectNav({ project }: { project: string }) {
  const t = useT()
  const place = usePlace()
  const view = useQuery(projectQuery(project))
  const data = view.data
  const summary = useProjectSummary(project)
  const advancedOpen = advancedSection.useValue()
  const advancedHolds = place.entry === "objects"

  const pageRow = ({ entry, to, icon: EntryIcon }: Entry) => (
    <SidebarItem
      key={entry}
      render={
        <Link to={to} params={{ project }} activeOptions={{ exact: true, includeSearch: false }} />
      }
      icon={<EntryIcon aria-hidden="true" />}
      current={place.entry === entry}
    >
      {t(`project.entry.${entry}`)}
    </SidebarItem>
  )

  return (
    <Sidebar aria-label={t("project.sections")}>
      <SidebarItem
        render={<Link to="/projects" activeOptions={{ exact: true, includeSearch: false }} />}
        icon={<ArrowLeftIcon aria-hidden="true" />}
      >
        {t("shell.projects")}
      </SidebarItem>
      <div className="flex shrink-0 flex-col gap-1 px-2">
        <div className="flex min-w-0 items-center gap-2">
          {summary?.icon ? <ProjectGlyph icon={summary.icon} className="size-5" /> : null}
          <Link
            to="/projects/$project"
            params={{ project }}
            activeOptions={{ exact: true, includeSearch: false }}
            className="min-w-0 flex-1 truncate type-name outline-none"
            translate="no"
            title={summary?.displayName ? project : undefined}
          >
            {projectTitle(summary, project)}
          </Link>
          <ProjectSwitcher current={project} />
        </div>
        {summary?.description ? (
          <p className="truncate text-xs text-tertiary" title={summary.description}>
            {summary.description}
          </p>
        ) : null}
      </div>
      <SearchRow />
      <SidebarContent>
        <SidebarGroup>{pages.map(pageRow)}</SidebarGroup>
        <SidebarSection title={t("column.label")}>
          {data ? (
            <Services project={project} data={data} />
          ) : (
            <div className="flex flex-col gap-1.5">
              {Array.from({ length: 4 }, (_, i) => (
                <Skeleton key={i} className="h-8 w-full" />
              ))}
            </div>
          )}
        </SidebarSection>
        <SidebarSection
          title={t("column.advanced")}
          open={advancedOpen || advancedHolds}
          // Held open by the page shown, a click is not a choice to remember.
          onOpenChange={(open) => {
            if (!advancedHolds) advancedSection.set(open)
          }}
        >
          {pageRow({
            entry: "objects",
            to: "/projects/$project/objects",
            icon: CubeIcon,
          })}
          {data && data.resources.length > 0 ? (
            <OtherObjects project={project} data={data} />
          ) : null}
        </SidebarSection>
      </SidebarContent>
      <SidebarCopyPill label={t("column.namespace")} value={project} copyLabel={t("common.copy")} />
      <SidebarFooter>
        <DockToggle />
        <ContextMenu />
      </SidebarFooter>
    </Sidebar>
  )
}

function Services({ project, data }: { project: string; data: Project }) {
  const t = useT()
  const place = usePlace()
  const groups = groupServices(data.services)
  if (groups.length === 0)
    return <p className="px-2 py-1 text-xs text-tertiary">{t("project.noServices")}</p>
  const row = (service: Service, child: boolean) => (
    <ServiceRow
      key={serviceId(service)}
      project={project}
      service={service}
      child={child}
      current={!place.entry && place.service === serviceId(service)}
    />
  )
  if (groups.length === 1) return <>{groups[0]?.services.map((s) => row(s, false))}</>
  return (
    <>
      {groups.map((group) => {
        const label = group.role ? (
          <Tooltip content={roleKube[group.role]}>
            <span>{t(`role.${group.role}`)}</span>
          </Tooltip>
        ) : (
          <span translate="no">{group.name}</span>
        )
        return (
          <SidebarMenu
            key={group.key}
            icon={
              group.role ? <RoleIcon role={group.role} /> : <FolderSimpleIcon aria-hidden="true" />
            }
            label={label}
            badge={<span className="type-nano text-tertiary">{group.services.length}</span>}
            active={group.services.some((s) => !place.entry && place.service === serviceId(s))}
            defaultOpen
          >
            {group.services.map((s) => row(s, true))}
          </SidebarMenu>
        )
      })}
    </>
  )
}

function ServiceRow({
  project,
  service,
  child,
  current,
}: {
  project: string
  service: Service
  child: boolean
  current: boolean
}) {
  const place = usePlace()
  const word = useStatusWord()
  const status = statusOf(service)
  return (
    <SidebarItem
      tone={child ? "child" : "default"}
      render={
        <Link
          {...serviceLink(project, service, {
            tab: place.entry ? undefined : place.tab,
          })}
          activeOptions={{ exact: true, includeSearch: false }}
        />
      }
      icon={<ServiceIcon service={service} size="sm" className="text-current" />}
      current={current}
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
}

function OtherObjects({ project, data }: { project: string; data: Project }) {
  const t = useT()
  const unused = data.resources.filter((r) => r.attribution.ownership === "orphan").length
  return (
    <SidebarMenu
      icon={<StackIcon aria-hidden="true" />}
      label={t("column.other")}
      badge={
        <span className="flex shrink-0 items-center gap-1.5 type-nano text-tertiary">
          {data.resources.length}
          {unused > 0 ? (
            <span title={t("resource.unusedCount", unused)} className="flex">
              <StatusDot health="warn" />
              <span className="sr-only">{t("resource.unusedCount", unused)}</span>
            </span>
          ) : null}
        </span>
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
              // The rows share one path and differ by `?object=`; the page may also have `?q`.
              activeOptions={{ exact: false }}
            />
          }
          trailing={
            r.attribution.ownership === "orphan" ? (
              <StatusPill health="warn" size="sm" dot={false}>
                {t("resource.unused")}
              </StatusPill>
            ) : (
              <span>{r.kind}</span>
            )
          }
        >
          <span translate="no">{r.name}</span>
        </SidebarItem>
      ))}
    </SidebarMenu>
  )
}

function objectKeyOf(kind: string, name: string): string | undefined {
  const address = addressOf(kind, name)
  return address ? objectKey(address) : undefined
}
