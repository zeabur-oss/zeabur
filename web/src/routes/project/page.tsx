import { useQuery } from "@tanstack/react-query"
import { getRouteApi } from "@tanstack/react-router"
import { useState } from "react"
import ListBulletsIcon from "~icons/ph/list-bullets"
import LightbulbIcon from "~icons/ph/lightbulb"
import TreeStructureIcon from "~icons/ph/tree-structure"

import type { Schemas } from "@/api/client"
import { projectGapsQuery, projectIncidentsQuery, projectQuery } from "@/api/queries"
import { Segmented } from "@/components/bits"
import { useDatabases } from "@/components/databases"
import { AddMenu } from "@/components/add/menu"
import { Freshness } from "@/components/freshness"
import { ProjectDeleting } from "@/components/project-deleting"
import { GapList } from "@/components/gaps"
import { IncidentRow, isTakenDown, takenDown } from "@/components/incident"
import { ProblemGroupRow } from "@/routes/project/problem-group"
import { refusedAmong, RefusedKinds } from "@/components/refused"
import { StatusPill } from "@/components/status"
import { Button } from "@zeabur/design-system/components/button"
import { Card, CardContent, CardHeader, CardTitle } from "@zeabur/design-system/components/card"
import { Drawer } from "@zeabur/design-system/components/drawer"
import { Skeleton } from "@zeabur/design-system/components/skeleton"
import { PageColumn } from "@zeabur/design-system/components/page-column"
import { PageHeader } from "@zeabur/design-system/components/page-header"
import { useT } from "@/i18n"
import { useHistoryIndex, useLocationText } from "@/lib/history"
import { PHONE, useMedia, WIDE } from "@/lib/media"
import { useProjectSummary } from "@/lib/project-name"
import { projectView } from "@/lib/stores"
import { groupProblems, type ProblemGroup } from "@/model/attention"
import { mapReadable } from "@/model/layout"
import { suggestionsOf } from "@/model/overview"
import { projectTitle } from "@/model/project"
import { needsAttention, statusOf, summarise, type Project, type Service } from "@/model/service"
import { mapLayout, ProjectMap } from "@/routes/project/architecture"
import { ProjectData, refusedPlaces, useProjectEvents } from "@/routes/project/data"
import { NextStep } from "@/routes/project/next-step"
import { ServicesTable } from "@/routes/project/services-table"
import {
  Activity,
  Alerts,
  Backups,
  Databases,
  ObjectStorageSection,
  Domains,
  SectionChips,
  Storage,
} from "@/routes/project/side"
import { projectSearch, type ProjectView } from "@/routes/search"

const route = getRouteApi("/projects/$project")

/** Up to this many services a phone shows each section inline rather than behind a button. */
const FEW = 4
/** Groups Needs attention lists before "Show all". */
const PROBLEMS_SHOWN = 5
const PROBLEMS_SHOWN_PHONE = 3

export function ProjectPage() {
  const { project } = route.useParams()
  return (
    <div className="flex min-h-full flex-1 flex-col">
      <ProjectData project={project}>
        {(data, unreadable) => (
          <ProjectBody project={project} data={data} unreadable={unreadable} />
        )}
      </ProjectData>
    </div>
  )
}

/** The canvas the map would get on this screen, before it is drawn: beside the activity column it
 * fills the window below the header and the problems; above the activity it keeps its least
 * height (`min-h-140`). */
function mapCanvas(broken: boolean): [number, number] {
  const main = document.querySelector("main")
  const width = main?.clientWidth ?? window.innerWidth
  if (!window.matchMedia(WIDE).matches) return [width, 560]
  return [width - 320, Math.max(560, window.innerHeight - (broken ? 240 : 100))]
}

/**
 * The map when it can be read fitted to the screen, else the list. The view chosen last in this
 * project stands only where the map is readable; `?view` always does. Decided once the databases,
 * which the map also draws, have loaded.
 */
function useView(project: string, data: Project): ProjectView {
  const search = route.useSearch()
  const databases = useDatabases(project)
  const whole = useQuery(projectQuery(project)).data ?? data
  const [decided, setDecided] = useState<ProjectView>()
  const decide = (): ProjectView => {
    if (window.matchMedia(PHONE).matches) return "list"
    const broken = data.services.some((s) => needsAttention(statusOf(s)))
    const laid = mapLayout(data, whole, databases.data?.items ?? [])
    if (!mapReadable(laid, ...mapCanvas(broken))) return "list"
    return projectView.get(project) ?? "map"
  }
  const initial = decided ?? decide()
  if (decided === undefined && !databases.isPending) setDecided(initial)
  return search.view ?? initial
}

function ProjectBody({
  project,
  data,
  unreadable,
}: {
  project: string
  data: Project
  unreadable: Schemas.UnreadableView[]
}) {
  const view = useView(project, data)
  const refused = refusedAmong(unreadable, refusedPlaces.services)
  const empty = data.services.length === 0 && refused.length === 0
  const deleting = useProjectSummary(project)?.deleting ?? false
  return (
    <>
      <Header project={project} data={data} view={empty ? undefined : view} />
      {empty ? (
        deleting ? null : (
          <NextStep project={project} />
        )
      ) : view === "map" ? (
        <MapView project={project} data={data} unreadable={unreadable} />
      ) : (
        <ListView project={project} data={data} unreadable={unreadable} />
      )}
    </>
  )
}

function Header({ project, data, view }: { project: string; data: Project; view?: ProjectView }) {
  const t = useT()
  const navigate = route.useNavigate()
  const document = useQuery(projectQuery(project))
  const details = useProjectSummary(project)
  const choose = (next: ProjectView) => {
    projectView.set(project, next)
    void navigate({
      search: (previous) => ({ ...previous, view: next, service: undefined }),
      replace: true,
    })
  }
  return (
    <header className="shrink-0 pt-6 pb-4 md:pt-8">
      <PageColumn>
        <PageHeader
          title={<span translate="no">{projectTitle(details, project)}</span>}
          badges={
            <>
              {details?.displayName ? (
                <span className="type-meta text-muted-foreground" translate="no">
                  {project}
                </span>
              ) : null}
              <HealthPill data={data} />
            </>
          }
          description={
            details?.description ? (
              <span className="whitespace-pre-line">{details.description}</span>
            ) : undefined
          }
          actions={
            <span className="flex flex-wrap items-center gap-3">
              <Freshness
                project={project}
                updatedAt={document.dataUpdatedAt}
                failed={document.isRefetchError}
              />
              <Suggestions project={project} data={data} />
              {view ? (
                <Segmented
                  label={t("project.view")}
                  value={view}
                  onChange={choose}
                  options={[
                    {
                      value: "map",
                      label: (
                        <>
                          <TreeStructureIcon aria-hidden="true" />
                          {t("project.viewMap")}
                        </>
                      ),
                    },
                    {
                      value: "list",
                      label: (
                        <>
                          <ListBulletsIcon aria-hidden="true" />
                          {t("project.viewList")}
                        </>
                      ),
                    },
                  ]}
                />
              ) : null}
              <AddMenu project={project} />
            </span>
          }
        />
        <ProjectDeleting project={project} className="mt-4" />
      </PageColumn>
    </header>
  )
}

function HealthPill({ data }: { data: Project }) {
  const t = useT()
  const summary = summarise(data)
  if (summary.services === 0) return null
  const count = (health: string) =>
    data.services.filter((s) => statusOf(s).health === health).length
  if (summary.health === "fail")
    return (
      <StatusPill health="fail">
        {count("warn") > 0
          ? [
              t("overview.failingCount", count("fail")),
              t("overview.degradedCount", count("warn")),
            ].join(t("common.sep"))
          : t("overview.failingCount", count("fail"))}
      </StatusPill>
    )
  if (summary.health === "warn")
    return <StatusPill health="warn">{t("overview.degradedCount", count("warn"))}</StatusPill>
  if (summary.health === "progress")
    return <StatusPill health="progress">{t("status.rollingOut")}</StatusPill>
  return <StatusPill health="ok">{t("overview.healthy")}</StatusPill>
}

function Suggestions({ project, data }: { project: string; data: Project }) {
  const t = useT()
  const [open, setOpen] = useState(false)
  const gaps = useQuery(projectGapsQuery(project))
  const items = gaps.data?.items ?? []
  if (gaps.isPending)
    return (
      <div aria-busy="true">
        <span className="sr-only">{t("project.suggestionsLoading")}</span>
        <Skeleton className="h-8 w-32" />
      </div>
    )
  if (items.length === 0) return null
  return (
    <>
      <Button variant="ghost" size="sm" onClick={() => setOpen(true)}>
        <LightbulbIcon aria-hidden="true" data-icon="inline-start" />
        {t("project.suggestions", suggestionsOf(items).length)}
      </Button>
      <Drawer
        open={open}
        onOpenChange={setOpen}
        title={t("gap.title")}
        description={t("project.suggestionsBody")}
        closeLabel={t("common.close")}
      >
        <GapList gaps={items} project={project} document={data} />
        {gaps.data && gaps.data.unreadable.length > 0 ? (
          <p className="type-meta text-muted-foreground">{t("gap.unreadable")}</p>
        ) : null}
      </Drawer>
    </>
  )
}

function Problems({ project, services }: { project: string; services: Service[] }) {
  const incidents = useQuery(projectIncidentsQuery(project)).data?.items ?? []
  const down = takenDown(incidents)
  const groups = groupProblems(services.filter((s) => !isTakenDown(down, project, s)))
  if (groups.length === 0 && incidents.length === 0) return null
  return <ProblemsCard project={project} incidents={incidents} groups={groups} />
}

function ProblemsCard({
  project,
  incidents,
  groups,
}: {
  project: string
  incidents: Schemas.Incident[]
  groups: ProblemGroup[]
}) {
  const t = useT()
  const broken = groups.reduce((sum, g) => sum + g.services.length, 0)
  return (
    <section aria-labelledby="project-problems">
      <Card variant="list">
        <CardHeader className="flex items-center">
          <CardTitle className="flex items-baseline">
            <h2 id="project-problems" className="type-heading">
              {t("attention.title")}
            </h2>
            <span className="ml-2.5 type-meta font-normal text-muted-foreground">
              {broken + incidents.length}
            </span>
          </CardTitle>
        </CardHeader>
        <CardContent>
          <ProblemList project={project} incidents={incidents} groups={groups} />
        </CardContent>
      </Card>
    </section>
  )
}

function ProblemList({
  project,
  incidents,
  groups,
}: {
  project: string
  incidents: Schemas.Incident[]
  groups: ProblemGroup[]
}) {
  const t = useT()
  const phone = useMedia(PHONE)
  const [all, setAll] = useState(false)
  const limit = phone ? PROBLEMS_SHOWN_PHONE : PROBLEMS_SHOWN
  const shown = all || groups.length <= limit + 1 ? groups : groups.slice(0, limit)
  return (
    <>
      <ul className="flex flex-col">
        {incidents.map((incident) => (
          <IncidentRow key={incident.id} incident={incident} showProject={false} />
        ))}
        {shown.map((group) => (
          <ProblemGroupRow key={group.key} project={project} group={group} />
        ))}
      </ul>
      {shown.length < groups.length || all ? (
        <div className="border-t border-rule px-2 py-1.5">
          <Button variant="ghost" size="xs" onClick={() => setAll(!all)}>
            {all ? t("services.showFewer") : t("services.showAll", groups.length)}
          </Button>
        </div>
      ) : null}
    </>
  )
}

function MapView({
  project,
  data,
  unreadable,
}: {
  project: string
  data: Project
  unreadable: Schemas.UnreadableView[]
}) {
  const search = route.useSearch()
  const navigate = route.useNavigate()
  const events = useProjectEvents(project)
  const wide = useMedia(WIDE)
  const select = (id?: string) =>
    void navigate({ search: (previous) => ({ ...previous, service: id }) })
  const broken = data.services.some((s) => needsAttention(statusOf(s)))
  const activity = (
    <div className="flex flex-col gap-7">
      <Alerts project={project} services={data.services} />
      <Activity
        project={project}
        services={data.services}
        events={events}
        unreadable={unreadable}
        framed={!wide}
      />
    </div>
  )
  return (
    <>
      {broken ? (
        <PageColumn className="mb-4 shrink-0">
          <Problems project={project} services={data.services} />
        </PageColumn>
      ) : null}
      <RefusedKinds unreadable={refusedAmong(unreadable, refusedPlaces.services)} />
      <ProjectMap project={project} data={data} selected={search.service} onSelect={select}>
        {wide ? (
          <aside className="w-80 shrink-0 overflow-y-auto border-l border-main px-5 py-5">
            {activity}
          </aside>
        ) : null}
      </ProjectMap>
      {wide ? null : <PageColumn className="mt-6 mb-12 shrink-0">{activity}</PageColumn>}
    </>
  )
}

function ListView({
  project,
  data,
  unreadable,
}: {
  project: string
  data: Project
  unreadable: Schemas.UnreadableView[]
}) {
  // After Back, Forward or a link, the filter box starts again from the URL.
  const entry = useHistoryIndex()
  const typed = useLocationText((search) => projectSearch.parse(search).q ?? "")
  const events = useProjectEvents(project)
  const phone = useMedia(PHONE)
  const refused = (resources: readonly string[]) => refusedAmong(unreadable, resources)
  const activity = (
    <Activity
      project={project}
      services={data.services}
      events={events}
      unreadable={unreadable}
      framed
    />
  )
  const services = (
    <div className="flex flex-col gap-3">
      <RefusedKinds unreadable={refused(refusedPlaces.services)} />
      <ServicesTable key={entry} project={project} data={data} typed={typed} />
    </div>
  )
  const side = (
    <>
      <Alerts project={project} services={data.services} />
      <Domains project={project} data={data} refused={refused(refusedPlaces.routes)} />
      <Storage project={project} data={data} refused={refused(refusedPlaces.claims)} />
      <Databases project={project} />
      <ObjectStorageSection project={project} />
      <Backups project={project} />
    </>
  )
  if (phone)
    return (
      <PageColumn gap="md" className="mb-12">
        <Problems project={project} services={data.services} />
        {services}
        {data.services.length > FEW ? (
          <SectionChips project={project} data={data} events={events} unreadable={unreadable} />
        ) : (
          <>
            {side}
            {activity}
          </>
        )}
      </PageColumn>
    )
  return (
    <PageColumn gap="md" className="mb-12">
      <Problems project={project} services={data.services} />
      {/* Activity sits under the services, so the two columns end close together. */}
      <div className="@container">
        <div className="overview-columns">
          <div className="flex min-w-0 flex-col">{services}</div>
          <div className="flex min-w-0 flex-col gap-7 empty:hidden">{side}</div>
          <div className="min-w-0">{activity}</div>
        </div>
      </div>
    </PageColumn>
  )
}
