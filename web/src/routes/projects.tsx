import { useQueries, useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { useId, useState } from "react"

import { isStatus, toProblem } from "@/api/client"
import { eventsQuery, projectQuery, projectsQuery } from "@/api/queries"
import { AdoptButton } from "@/components/adopt"
import { Problem } from "@/components/problem"
import { ProjectIcon } from "@/components/project-icon"
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@zeabur/design-system/components/empty"
import { Label } from "@zeabur/design-system/components/label"
import { Skeleton } from "@zeabur/design-system/components/skeleton"
import { Switch } from "@zeabur/design-system/components/switch"
import { useT } from "@/i18n"
import { otherNamespaces } from "@/model/project"
import { summarise } from "@/model/service"
import { ProjectCard } from "@/components/project-card"
import { Illustration } from "@zeabur/design-system/components/illustration"
import { PageHeader } from "@zeabur/design-system/components/page-header"

export function ProjectsPage() {
  const t = useT()
  const projects = useQuery(projectsQuery)
  const [showSystem, setShowSystem] = useState(false)
  const switchId = useId()
  const items = projects.data?.items ?? []
  const adopted = items.filter((p) => p.adopted)
  const other = otherNamespaces(items, showSystem)
  // Every adopted project at once: read on load, not polled (five-second polls of every project
  // would multiply the cluster reads). TODO(part C): use the list's health counts instead.
  const views = useQueries({
    queries: adopted.map((p) => projectQuery(p.name, { poll: false })),
  })
  const events = useQueries({
    queries: adopted.map((p) => eventsQuery(p.name, { poll: false })),
  })

  return (
    <div className="mx-auto flex w-full max-w-7xl flex-col gap-6 px-4 py-6 md:px-8 md:py-8">
      <PageHeader title={t("projects.title")} />
      {projects.isPending ? (
        <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
          {Array.from({ length: 3 }, (_, i) => (
            <Skeleton key={i} variant="card" className="h-44" />
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
          <ul className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
            {adopted.map((p, i) => {
              const data = views[i]?.data
              return (
                <li key={p.name} className="flex">
                  <ProjectCard
                    name={p.name}
                    details={p}
                    project={data}
                    summary={data ? summarise(data) : undefined}
                    events={events[i]?.data?.items ?? []}
                  />
                </li>
              )
            })}
          </ul>
          <section className="flex flex-col gap-3">
            <div className="flex flex-wrap items-end justify-between gap-2">
              <h2 className="text-base font-medium">{t("projects.other")}</h2>
              <div className="flex items-center gap-2">
                <Switch id={switchId} checked={showSystem} onCheckedChange={setShowSystem} />
                <Label htmlFor={switchId}>{t("projects.showSystem")}</Label>
              </div>
            </div>
            <ul className="divide-y overflow-hidden rounded-lg border bg-card shadow-xs">
              {other.map((p) => (
                <li key={p.name} className="flex items-center gap-3 pr-3">
                  <Link
                    to="/projects/$project"
                    params={{ project: p.name }}
                    className="flex min-w-0 flex-1 items-center gap-3 px-4 py-2.5 text-sm outline-none hover:bg-hover"
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
