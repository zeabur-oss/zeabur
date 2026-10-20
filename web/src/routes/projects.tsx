import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { useId, useState } from "react"

import { isStatus, toProblem } from "@/api/client"
import { projectsQuery } from "@/api/queries"
import { AdoptButton } from "@/components/adopt"
import { Problem } from "@/components/problem"
import { ProjectCollection, ProjectViewSwitch } from "@/components/project-card"
import { ProjectIcon } from "@/components/project-icon"
import { useT } from "@/i18n"
import { otherNamespaces } from "@/model/project"
import { countsHealth, healthRank } from "@/model/service"
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@zeabur/design-system/components/empty"
import { Illustration } from "@zeabur/design-system/components/illustration"
import { Label } from "@zeabur/design-system/components/label"
import { PageHeader } from "@zeabur/design-system/components/page-header"
import { Skeleton } from "@zeabur/design-system/components/skeleton"
import { Switch } from "@zeabur/design-system/components/switch"

export function ProjectsPage() {
  const t = useT()
  const projects = useQuery(projectsQuery)
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
    <div className="mx-auto flex w-full max-w-6xl flex-col gap-8 px-4 pt-6 pb-12 md:box-content md:px-8 md:pt-10">
      <PageHeader title={t("projects.title")} actions={<ProjectViewSwitch />} />
      {projects.isPending ? (
        <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
          {Array.from({ length: 3 }, (_, i) => (
            <Skeleton key={i} variant="card" className="h-42" />
          ))}
        </div>
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
            <ProjectCollection projects={adopted} label={t("projects.title")} />
          ) : (
            <Empty size="sm">
              <EmptyHeader>
                <EmptyMedia>
                  <Illustration kind="empty" seed="projects" />
                </EmptyMedia>
                <EmptyDescription>{t("cluster.noProjects")}</EmptyDescription>
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
            <ul className="flex flex-col divide-y divide-main overflow-hidden rounded-xl border border-main bg-card">
              {other.map((p) => (
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
                  {p.adoptable ? <AdoptButton namespace={p.name} adopted={false} /> : null}
                </li>
              ))}
            </ul>
          </section>
        </>
      )}
    </div>
  )
}
