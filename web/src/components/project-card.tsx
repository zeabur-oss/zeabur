import { useQueries } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import type { ReactNode } from "react"
import ListBulletsIcon from "~icons/ph/list-bullets"
import SquaresFourIcon from "~icons/ph/squares-four"

import type { Schemas } from "@/api/client"
import { projectQuery } from "@/api/queries"
import { ProjectGlyph } from "@/components/shell/project-glyph"
import { StatusDot } from "@/components/status"
import { useT } from "@/i18n"
import { useSeen } from "@/lib/in-view"
import { projectsAsList } from "@/lib/stores"
import { domainsOf, type Domain, type Health, type Project } from "@/model/service"
import { Skeleton } from "@zeabur/design-system/components/skeleton"
import { Tabs, TabsList, TabsTrigger } from "@zeabur/design-system/components/tabs"
import { Tooltip } from "@zeabur/design-system/components/tooltip"

type ProjectItem = Schemas["ProjectSummary"]

/** Fewest labels (`shop.example.com` before `api.shop.example.com`), then shortest, then by name. */
function primaryDomain(document: Project): Domain | undefined {
  const labels = (host: string) => host.split(".").length
  return document.services
    .flatMap((service) => domainsOf(document, service))
    .toSorted(
      (a, b) =>
        labels(a.host) - labels(b.host) ||
        a.host.length - b.host.length ||
        a.host.localeCompare(b.host),
    )[0]
}

interface CardProps {
  project: ProjectItem
  document?: Project
  documentPending?: boolean
}

function ProjectTile({ project }: { project: ProjectItem }) {
  return (
    <span className="flex size-8 shrink-0 items-center justify-center rounded-lg border border-main bg-secondary text-main">
      {project.icon ? (
        <ProjectGlyph icon={project.icon} className="size-4" />
      ) : (
        <span className="text-xs font-semibold" aria-hidden="true">
          {(project.displayName ?? project.name).charAt(0).toUpperCase()}
        </span>
      )}
    </span>
  )
}

function ServicesHealth({ project }: { project: ProjectItem }) {
  const t = useT()
  const h = project.health
  if (!project.services || !h) return <span className="text-xs text-secondary">—</span>
  if (h.services === 0)
    return <span className="text-xs text-secondary">{t("home.noServices")}</span>
  const dots: Health[] = []
  if (h.healthy > 0) dots.push("ok")
  if (h.failing > 0) dots.push("fail")
  if (h.warning > 0) dots.push("warn")
  if (h.progressing > 0) dots.push("progress")
  const words = [
    h.healthy > 0 ? t("project.healthyOf", h.healthy, h.services) : "",
    h.failing > 0 ? t("overview.failingCount", h.failing) : "",
    h.warning > 0 ? t("overview.degradedCount", h.warning) : "",
    h.progressing > 0 ? t("home.rollingOut", h.progressing) : "",
  ].filter(Boolean)
  return (
    <Tooltip content={words.join(t("common.sep"))}>
      <span className="flex items-center gap-1.5 text-xs text-main">
        <span className="flex items-center gap-0.5">
          {dots.map((health) => (
            <StatusDot key={health} health={health} />
          ))}
        </span>
        <span className="tabular-nums">{t("project.healthyOf", h.healthy, h.services)}</span>
      </span>
    </Tooltip>
  )
}

function DomainValue({ document, documentPending }: Omit<CardProps, "project">) {
  const domain = document ? primaryDomain(document) : undefined
  if (domain)
    return (
      <span
        className="block truncate font-mono text-xs text-main"
        translate="no"
        title={domain.host}
      >
        {domain.host}
      </span>
    )
  if (documentPending) return <Skeleton className="h-4 w-28" />
  return <span className="text-xs text-secondary">—</span>
}

function InfoRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex h-5 items-center justify-between gap-x-3">
      <span className="shrink-0 text-xs font-medium text-secondary">{label}</span>
      <div className="flex min-w-0 justify-end">{children}</div>
    </div>
  )
}

function ProjectCard({ project, document, documentPending }: CardProps) {
  const t = useT()
  return (
    <Link
      to="/projects/$project"
      params={{ project: project.name }}
      className="group/card flex min-h-42 w-full min-w-0 flex-col rounded-xl border border-main bg-card p-4 pt-3 transition-shadow outline-none hover:shadow-lg"
    >
      <span className="flex h-8 items-center gap-x-2.5">
        <ProjectTile project={project} />
        <span className="min-w-0 flex-1 truncate type-heading text-main" translate="no">
          {project.displayName ?? project.name}
        </span>
      </span>
      <span className="mt-2 block h-4 truncate text-xs text-tertiary">{project.description}</span>
      <span className="min-h-2 flex-1" />
      <span className="flex flex-col gap-y-2.5">
        <InfoRow label={t("column.namespace")}>
          <span className="truncate font-mono text-xs text-main" translate="no">
            {project.name}
          </span>
        </InfoRow>
        <InfoRow label={t("metric.services")}>
          <ServicesHealth project={project} />
        </InfoRow>
        <InfoRow label={t("table.domain")}>
          <DomainValue document={document} documentPending={documentPending} />
        </InfoRow>
      </span>
    </Link>
  )
}

function ProjectListRow({ project, document, documentPending }: CardProps) {
  return (
    <Link
      to="/projects/$project"
      params={{ project: project.name }}
      className="flex min-h-16 w-full items-center gap-x-3 px-4 py-2 transition-colors outline-none hover:bg-v2-surface-hover"
    >
      <ProjectTile project={project} />
      <span className="flex min-w-0 flex-1 flex-col">
        <span className="truncate text-sm font-semibold text-main" translate="no">
          {project.displayName ?? project.name}
        </span>
        {project.description ? (
          <span className="truncate text-xs text-tertiary">{project.description}</span>
        ) : null}
      </span>
      <span
        className="hidden w-40 shrink-0 truncate font-mono text-xs text-main @3xl:block"
        translate="no"
      >
        {project.name}
      </span>
      <span className="hidden w-36 shrink-0 @xl:block">
        <ServicesHealth project={project} />
      </span>
      <span className="hidden w-48 shrink-0 @4xl:block">
        <DomainValue document={document} documentPending={documentPending} />
      </span>
    </Link>
  )
}

export function ProjectViewSwitch() {
  const t = useT()
  const list = projectsAsList.useValue()
  return (
    <Tabs value={list ? "list" : "grid"} onValueChange={(v) => projectsAsList.set(v === "list")}>
      <TabsList aria-label={t("projects.view")}>
        <TabsTrigger value="grid" aria-label={t("projects.viewGrid")}>
          <SquaresFourIcon aria-hidden="true" />
        </TabsTrigger>
        <TabsTrigger value="list" aria-label={t("projects.viewList")}>
          <ListBulletsIcon aria-hidden="true" />
        </TabsTrigger>
      </TabsList>
    </Tabs>
  )
}

export function ProjectCollection({ projects, label }: { projects: ProjectItem[]; label: string }) {
  const list = projectsAsList.useValue()
  const [ref, seen] = useSeen<HTMLDivElement>()
  const documents = useQueries({
    queries: projects.map((p) => ({ ...projectQuery(p.name, { poll: false }), enabled: seen })),
  })
  const item = (project: ProjectItem, i: number) => ({
    project,
    document: documents[i]?.data,
    documentPending: seen && (documents[i]?.isPending ?? false),
  })
  return (
    <div ref={ref} className="@container w-full">
      {list ? (
        <ul
          aria-label={label}
          className="flex w-full flex-col divide-y divide-main overflow-hidden rounded-xl border border-main bg-card"
        >
          {projects.map((p, i) => (
            <li key={p.name}>
              <ProjectListRow {...item(p, i)} />
            </li>
          ))}
        </ul>
      ) : (
        <ul
          aria-label={label}
          className="grid w-full grid-cols-1 gap-4 @2xl:grid-cols-2 @5xl:grid-cols-3"
        >
          {projects.map((p, i) => (
            <li key={p.name} className="flex">
              <ProjectCard {...item(p, i)} />
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}
