import { useQueries } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { useEffect, useRef, useState, type ReactNode } from "react"
import CaretDownIcon from "~icons/ph/caret-down"
import CaretUpDownIcon from "~icons/ph/caret-up-down"
import CaretUpIcon from "~icons/ph/caret-up"
import ListBulletsIcon from "~icons/ph/list-bullets"
import MagnifyingGlassIcon from "~icons/ph/magnifying-glass"
import SquaresFourIcon from "~icons/ph/squares-four"

import type { Schemas } from "@/api/client"
import { projectQuery } from "@/api/queries"
import { ProjectGlyph } from "@/components/shell/project-glyph"
import { StatusDot } from "@/components/status"
import { useT } from "@/i18n"
import { useSeen } from "@/lib/in-view"
import type { ProjectSortKey, ProjectsSearch } from "@/routes/search"
import { projectsAsList } from "@/lib/stores"
import { cn } from "@/lib/utils"
import { compareProjects, matchesProject, restartsIn } from "@/model/project"
import { domainsOf, type Domain, type Health, type Project } from "@/model/service"
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
} from "@zeabur/design-system/components/empty"
import { Illustration } from "@zeabur/design-system/components/illustration"
import { Kbd } from "@zeabur/design-system/components/kbd"
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

function ProjectTile({ project, small }: { project: ProjectItem; small?: boolean }) {
  return (
    <span
      className={cn(
        "flex shrink-0 items-center justify-center border border-main bg-secondary text-main",
        small ? "size-6 rounded-md" : "size-8 rounded-lg",
      )}
    >
      {project.icon ? (
        <ProjectGlyph icon={project.icon} className={small ? "size-3.5" : "size-4"} />
      ) : (
        <span className={cn("font-semibold", small ? "type-micro" : "text-xs")} aria-hidden="true">
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
      <div className="flex min-w-0 flex-1 justify-end">{children}</div>
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
        {project.displayName && project.displayName !== project.name ? (
          <InfoRow label={t("column.namespace")}>
            <span className="truncate font-mono text-xs text-main" translate="no">
              {project.name}
            </span>
          </InfoRow>
        ) : null}
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

const columns = {
  namespace: "hidden w-40 shrink-0 @3xl:block",
  services: "w-36 shrink-0",
  restarts: "hidden w-20 shrink-0 text-right @xl:block",
  domain: "hidden w-48 shrink-0 pl-3 @4xl:block",
}

function ProjectListRow({ project, document, documentPending }: CardProps) {
  const restarts = restartsIn(project)
  return (
    <Link
      to="/projects/$project"
      params={{ project: project.name }}
      className="flex h-10 w-full items-center gap-x-3 px-4 transition-colors outline-none hover:bg-v2-surface-hover"
    >
      <span className="flex min-w-0 flex-1 items-center gap-x-2.5">
        <ProjectTile project={project} small />
        <span className="min-w-0 truncate text-sm font-medium text-main" translate="no">
          {project.displayName ?? project.name}
        </span>
        {project.description ? (
          <span className="min-w-0 flex-1 basis-0 truncate text-xs text-tertiary">
            {project.description}
          </span>
        ) : null}
      </span>
      <span
        className={cn(columns.namespace, "truncate font-mono text-xs text-main")}
        translate="no"
      >
        {project.name}
      </span>
      <span className={columns.services}>
        <ServicesHealth project={project} />
      </span>
      <span
        className={cn(
          columns.restarts,
          "text-xs tabular-nums",
          restarts ? "text-main" : "text-secondary",
        )}
      >
        {restarts ?? "—"}
      </span>
      <span className={columns.domain}>
        <DomainValue document={document} documentPending={documentPending} />
      </span>
    </Link>
  )
}

function ListHeader({
  sort,
  descending,
  onSort,
}: {
  sort: ProjectSortKey
  descending: boolean
  onSort: (key: ProjectSortKey) => void
}) {
  const t = useT()
  const sortable = (key: ProjectSortKey, label: string) => {
    const active = sort === key
    const SortIcon = active ? (descending ? CaretUpIcon : CaretDownIcon) : CaretUpDownIcon
    return (
      <button
        type="button"
        onClick={() => onSort(key)}
        aria-pressed={active}
        className="inline-flex items-center gap-1 rounded outline-none hover:text-foreground"
      >
        {label}
        <SortIcon className={cn("size-3", !active && "opacity-40")} aria-hidden="true" />
      </button>
    )
  }
  return (
    <div className="flex h-8 items-center gap-x-3 border-b border-main bg-muted/30 px-4 type-meta font-medium whitespace-nowrap text-muted-foreground">
      <span className="min-w-0 flex-1">{sortable("name", t("projects.columnName"))}</span>
      <span className={columns.namespace}>{t("column.namespace")}</span>
      <span className={columns.services}>{sortable("health", t("metric.services"))}</span>
      <span className={columns.restarts}>{t("table.restarts")}</span>
      <span className={columns.domain}>{t("table.domain")}</span>
    </div>
  )
}

function useSlashFocus() {
  const ref = useRef<HTMLInputElement>(null)
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "/" || event.metaKey || event.ctrlKey || event.altKey) return
      const target = event.target
      if (
        target instanceof HTMLElement &&
        (target.isContentEditable || target.closest("input, textarea, select"))
      )
        return
      event.preventDefault()
      ref.current?.focus()
    }
    document.addEventListener("keydown", onKey)
    return () => document.removeEventListener("keydown", onKey)
  }, [])
  return ref
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

/** `filter` adds a filter field above the collection; `/` focuses it. The filter and the order
 * are the page's search (`?q`, `?sort`, `?desc`). */
export function ProjectCollection({
  projects,
  label,
  filter,
  search,
  onSearch,
}: {
  projects: ProjectItem[]
  label: string
  filter?: boolean
  search: ProjectsSearch
  onSearch: (patch: ProjectsSearch) => void
}) {
  const t = useT()
  const list = projectsAsList.useValue()
  const [ref, seen] = useSeen<HTMLDivElement>()
  const [query, setQuery] = useState(filter ? (search.q ?? "") : "")
  const sort = search.sort ?? "health"
  const descending = search.desc === true
  const input = useSlashFocus()
  const documents = useQueries({
    queries: projects.map((p) => ({ ...projectQuery(p.name, { poll: false }), enabled: seen })),
  })
  const items = projects.map((project, i) => ({
    project,
    document: documents[i]?.data,
    documentPending: seen && (documents[i]?.isPending ?? false),
  }))
  const needle = query.trim().toLowerCase()
  const shown = items
    .filter(({ project, document }) =>
      matchesProject(
        project,
        document ? document.services.flatMap((s) => domainsOf(document, s).map((d) => d.host)) : [],
        needle,
      ),
    )
    .toSorted((a, b) => (list ? compareProjects(a.project, b.project, sort, descending) : 0))
  const onSort = (key: ProjectSortKey) =>
    onSearch(
      key === sort
        ? { desc: descending ? undefined : true }
        : { sort: key === "health" ? undefined : key, desc: undefined },
    )
  return (
    <div ref={ref} className="@container flex w-full flex-col gap-3">
      {filter ? (
        <label className="flex h-8 w-full items-center gap-2 rounded-lg border bg-background px-2.5 focus-within:border-ring sm:w-64">
          <MagnifyingGlassIcon className="size-4 text-muted-foreground" aria-hidden="true" />
          <input
            ref={input}
            type="search"
            value={query}
            onChange={(event) => {
              setQuery(event.target.value)
              onSearch({ q: event.target.value || undefined })
            }}
            onKeyDown={(event) => {
              if (event.key === "Escape" && query) {
                event.stopPropagation()
                setQuery("")
                onSearch({ q: undefined })
              }
            }}
            aria-label={t("projects.filter")}
            placeholder={t("projects.filter")}
            aria-keyshortcuts="/"
            className="w-full bg-transparent text-sm outline-none placeholder:text-muted-foreground"
          />
          {query ? null : <Kbd keys="/" aria-hidden="true" className="max-md:hidden" />}
        </label>
      ) : null}
      {shown.length === 0 ? (
        <Empty size="sm">
          <EmptyHeader>
            <EmptyMedia>
              <Illustration kind="no-results" seed="projects" />
            </EmptyMedia>
            <EmptyDescription>{t("projects.noMatch", query.trim())}</EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : list ? (
        <div className="w-full overflow-hidden rounded-xl border border-main bg-card">
          <ListHeader sort={sort} descending={descending} onSort={onSort} />
          <ul aria-label={label} className="flex w-full flex-col divide-y divide-main">
            {shown.map((item) => (
              <li key={item.project.name}>
                <ProjectListRow {...item} />
              </li>
            ))}
          </ul>
        </div>
      ) : (
        <ul
          aria-label={label}
          className="grid w-full grid-cols-1 gap-4 @2xl:grid-cols-2 @5xl:grid-cols-3"
        >
          {shown.map((item) => (
            <li key={item.project.name} className="flex">
              <ProjectCard {...item} />
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}
