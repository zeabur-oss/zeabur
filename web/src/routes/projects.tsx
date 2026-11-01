import { useQuery } from "@tanstack/react-query"
import { getRouteApi, Link, useNavigate } from "@tanstack/react-router"
import { useId, useState } from "react"
import CaretDownIcon from "~icons/ph/caret-down"
import MagnifyingGlassIcon from "~icons/ph/magnifying-glass"

import { isStatus, toProblem } from "@/api/client"
import { projectsQuery } from "@/api/queries"
import { AdoptMenu } from "@/components/adopt"
import { NewProjectButton } from "@/components/new-project"
import { Problem } from "@/components/problem"
import { ProjectCollection, ProjectViewSwitch } from "@/components/project-card"
import { ProjectIcon } from "@/components/project-icon"
import { useT } from "@/i18n"
import { projectsAsList } from "@/lib/stores"
import { otherNamespaces } from "@/model/project"
import { countsHealth, healthRank } from "@/model/service"
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@zeabur-oss/design-system/components/empty"
import { Button } from "@zeabur-oss/design-system/components/button"
import { Illustration } from "@zeabur-oss/design-system/components/illustration"
import { Label } from "@zeabur-oss/design-system/components/label"
import { PageHeader } from "@zeabur-oss/design-system/components/page-header"
import { Skeleton } from "@zeabur-oss/design-system/components/skeleton"
import { Switch } from "@zeabur-oss/design-system/components/switch"
import { PageColumn } from "@zeabur-oss/design-system/components/page-column"

const route = getRouteApi("/projects")

/** Above this many namespaces the list gets a filter and a bound. */
const FILTER_AT = 10
const NAMESPACES_SHOWN = 20

type Namespace = ReturnType<typeof otherNamespaces>[number]

/** Namespaces that are not projects yet, each with Adopt. Many get a filter and a bound; none
 * is a quiet line rather than an empty box. */
export function OtherNamespaces({
  namespaces,
  limit = NAMESPACES_SHOWN,
}: {
  namespaces: readonly Namespace[]
  limit?: number
}) {
  const t = useT()
  const [query, setQuery] = useState("")
  const [all, setAll] = useState(false)
  if (namespaces.length === 0)
    return <p className="type-meta text-muted-foreground">{t("projects.noOther")}</p>
  const needle = query.trim().toLowerCase()
  const matching = needle
    ? namespaces.filter((p) => p.name.toLowerCase().includes(needle))
    : namespaces
  const shown = all ? matching : matching.slice(0, limit)
  return (
    <div className="flex flex-col gap-3">
      {namespaces.length > FILTER_AT ? (
        <label className="flex h-8 w-full items-center gap-2 rounded-lg border bg-background px-2.5 focus-within:border-ring sm:w-64">
          <MagnifyingGlassIcon className="size-4 text-muted-foreground" aria-hidden="true" />
          <input
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            aria-label={t("projects.filterOther")}
            placeholder={t("projects.filterOther")}
            className="w-full bg-transparent text-sm outline-none placeholder:text-muted-foreground"
          />
        </label>
      ) : null}
      {matching.length === 0 ? (
        <p className="type-meta text-muted-foreground">
          {t("projects.noOtherMatch", query.trim())}
        </p>
      ) : (
        <ul
          aria-label={t("projects.other")}
          className="flex flex-col divide-y divide-main overflow-hidden rounded-xl border border-main bg-card"
        >
          {shown.map((p) => (
            <li key={p.name} className="flex items-center gap-3 pr-3">
              <Link
                to="/projects/$project"
                params={{ project: p.name }}
                className="flex min-w-0 flex-1 items-center gap-3 px-4 py-3 text-sm outline-none hover:bg-v2-surface-hover"
              >
                <ProjectIcon icon={p.icon} />
                <span className="truncate font-medium" translate="no">
                  {p.name}
                </span>
              </Link>
              {p.adoptable ? <AdoptMenu namespace={p.name} /> : null}
            </li>
          ))}
        </ul>
      )}
      {matching.length > limit ? (
        <div>
          <Button variant="ghost" size="sm" onClick={() => setAll(!all)} aria-expanded={all}>
            <CaretDownIcon aria-hidden="true" data-icon="inline-start" />
            {all ? t("cluster.fewer") : t("cluster.more", matching.length - limit)}
          </Button>
        </div>
      ) : null}
    </div>
  )
}

export function ProjectsPage() {
  const t = useT()
  const search = route.useSearch()
  const navigate = useNavigate({ from: "/projects" })
  const projects = useQuery(projectsQuery)
  const list = projectsAsList.useValue()
  const [showSystem, setShowSystem] = useState(false)
  const switchId = useId()
  const items = projects.data?.items ?? []
  const rank = (p: (typeof items)[number]) =>
    p.health ? healthRank[countsHealth(p.health)] : healthRank.ok
  const adopted = items
    .filter((p) => p.adopted)
    .toSorted((a, b) => rank(a) - rank(b) || a.name.localeCompare(b.name))
  const other = otherNamespaces(items, showSystem)

  return (
    <PageColumn gap="lg" className="mt-6 mb-12 md:mt-10">
      <PageHeader
        title={t("projects.title")}
        actions={
          <span className="flex items-center gap-2">
            <ProjectViewSwitch />
            {projects.data ? (
              <NewProjectButton
                canCreate={projects.data.canCreate}
                taken={items.map((p) => p.name)}
              />
            ) : null}
          </span>
        }
      />
      {projects.isPending ? (
        list ? (
          <Skeleton variant="card" className="h-50" />
        ) : (
          <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
            {Array.from({ length: 3 }, (_, i) => (
              <Skeleton key={i} variant="card" className="h-42" />
            ))}
          </div>
        )
      ) : projects.isError ? (
        isStatus(projects.error, 403) ? (
          <Empty variant="outline">
            <EmptyHeader>
              <EmptyMedia>
                <Illustration kind="forbidden" />
              </EmptyMedia>
              <EmptyTitle>{t("projects.forbiddenTitle")}</EmptyTitle>
              <EmptyDescription>{toProblem(projects.error).detail}</EmptyDescription>
            </EmptyHeader>
          </Empty>
        ) : (
          <Problem error={projects.error} />
        )
      ) : (
        <>
          {adopted.length > 0 ? (
            <ProjectCollection
              projects={adopted}
              label={t("projects.title")}
              filter
              search={search}
              onSearch={(patch) =>
                void navigate({ search: (previous) => ({ ...previous, ...patch }), replace: true })
              }
            />
          ) : (
            <Empty size="sm">
              <EmptyHeader>
                <EmptyMedia>
                  <Illustration kind="empty" seed="projects" />
                </EmptyMedia>
                <EmptyTitle>{t("cluster.noProjects")}</EmptyTitle>
                <EmptyDescription>{t("projects.noneBody")}</EmptyDescription>
              </EmptyHeader>
            </Empty>
          )}
          <section className="flex flex-col gap-3">
            <div className="flex flex-wrap items-end justify-between gap-2">
              <h2 className="type-heading">{t("projects.other")}</h2>
              <div className="flex items-center gap-2">
                <Switch id={switchId} checked={showSystem} onCheckedChange={setShowSystem} />
                <Label htmlFor={switchId}>{t("projects.showSystem")}</Label>
              </div>
            </div>
            <OtherNamespaces namespaces={other} />
          </section>
        </>
      )}
    </PageColumn>
  )
}
