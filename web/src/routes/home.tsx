import { useQueries, useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import type { ReactNode } from "react"
import CaretRightIcon from "~icons/ph/caret-right"

import { isStatus, type Schemas } from "@/api/client"
import {
  clusterIncidentsQuery,
  clusterQuery,
  nodesQuery,
  projectQuery,
  projectsQuery,
  recentWarningsQuery,
  upgradesQuery,
} from "@/api/queries"
import { Ago } from "@/components/bits"
import { LoadingRows } from "@/components/page"
import { Problem } from "@/components/problem"
import { IncidentRow, isTakenDown, takenDown, type Incident } from "@/components/incident"
import { ProblemRow } from "@/components/problem-row"
import { StatusDot, StatusPill } from "@/components/status"
import { buttonVariants } from "@zeabur/design-system/components/button"
import { Card, CardContent, CardHeader, CardTitle } from "@zeabur/design-system/components/card"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur/design-system/components/collapsible"
import { Skeleton } from "@zeabur/design-system/components/skeleton"
import { useT } from "@/i18n"
import { serviceLink } from "@/lib/place"
import { originOf, originRank, type Origin } from "@/model/origin"
import {
  byHealth,
  countsHealth,
  healthRank,
  needsAttention,
  statusOf,
  type Event,
  type Service,
} from "@/model/service"
import { ProjectCollection, ProjectViewSwitch } from "@/components/project-card"
import { Forbidden, overviewRole } from "@/routes/cluster/forbidden"
import { useAttention } from "@/routes/cluster/attention"
import { useUpgradeBadge } from "@/routes/cluster/upgrade"
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@zeabur/design-system/components/empty"
import { Illustration } from "@zeabur/design-system/components/illustration"
import { PageHeader } from "@zeabur/design-system/components/page-header"

type ProjectItem = Schemas["ProjectSummary"]

const PROJECTS_SHOWN = 9
const WARNINGS_SHOWN = 5

export function HomePage() {
  const t = useT()
  const projects = useQuery(projectsQuery)
  const adopted = projects.data?.items.filter((p) => p.adopted) ?? []
  const problems = useProblems(adopted)
  return (
    <div className="mx-auto flex w-full max-w-6xl flex-col gap-8 px-4 pt-6 pb-12 type-body md:box-content md:px-8 md:pt-10">
      <div className="flex flex-col gap-2">
        <PageHeader title={t("home.title")} />
        <StateLine adopted={adopted} loaded={projects.isSuccess} problems={problems} />
      </div>
      {/* Problems come in after the projects; showing both at once keeps them from pushing the
          projects down. */}
      {projects.isPending || problems.pending ? (
        <LoadingRows label={t("common.loading")} rows={4} />
      ) : (
        <>
          <ProblemsSection problems={problems} />
          <ProjectsSection adopted={adopted} error={projects.isError ? projects.error : null} />
          <WarningsSection adopted={adopted} />
        </>
      )}
    </div>
  )
}

interface Found {
  project: string
  service: Service
}

interface Problems {
  /** Still reading the documents of the projects in trouble. */
  pending: boolean
  incidents: Incident[]
  /** Cluster warnings that take nothing down yet: a count and a link. */
  notices: number
  byOrigin: { origin: Origin; items: Found[] }[]
  rolling: Found[]
  count: number
}

/** Every problem once, by where it comes from: a cause in the cluster shows once, with the
 * services it takes down under it. */
function useProblems(adopted: ProjectItem[]): Problems {
  const attention = useAttention().filter((i) => i.health === "fail" || i.health === "warn")
  const incidentList = useQuery(clusterIncidentsQuery)
  const incidents = incidentList.data?.items ?? []
  const down = takenDown(incidents)
  const troubled = adopted.filter((p) =>
    (p.services ?? []).some((s) => s.health !== "ok" && s.health !== "idle"),
  )
  const documents = useQueries({
    queries: troubled.map((p) => projectQuery(p.name)),
  })
  const found: Found[] = documents.flatMap((d, i) =>
    (d.data?.services ?? []).map((service) => ({ project: troubled[i]?.name ?? "", service })),
  )
  const origins = new Map<Origin, Found[]>()
  const rolling: Found[] = []
  for (const f of found) {
    const status = statusOf(f.service)
    if (status.health === "progress") {
      rolling.push(f)
      continue
    }
    if (!needsAttention(status)) continue
    if (isTakenDown(down, f.project, f.service) || f.service.diagnoses?.some((d) => d.incident))
      continue
    const first = f.service.diagnoses?.[0]
    const origin = first ? originOf(first).origin : "unknown"
    origins.set(origin, [...(origins.get(origin) ?? []), f])
  }
  const byOrigin = [...origins.entries()]
    .map(([origin, items]) => ({ origin, items: items.toSorted(byFound) }))
    .toSorted((a, b) => originRank[a.origin] - originRank[b.origin])
  const explained = new Set(incidents.map((i) => i.id))
  const notices = attention.filter(
    (i) => !(i.node && explained.has(`incidents/node/${i.node}`)),
  ).length
  return {
    pending: documents.some((d) => d.isPending) || incidentList.isPending,
    incidents,
    notices,
    byOrigin,
    rolling: rolling.toSorted(byFound),
    count: incidents.length + byOrigin.reduce((n, g) => n + g.items.length, 0),
  }
}

const byFound = (a: Found, b: Found) =>
  byHealth(a.service, b.service) ||
  a.project.localeCompare(b.project) ||
  a.service.name.localeCompare(b.service.name)

function StateLine({
  adopted,
  loaded,
  problems,
}: {
  adopted: ProjectItem[]
  loaded: boolean
  problems: Problems
}) {
  const t = useT()
  if (!loaded || problems.pending) return <Skeleton className="h-5 w-72" />
  const services = adopted.reduce((n, p) => n + (p.health?.services ?? 0), 0)
  const parts =
    problems.count > 0
      ? [t("home.state.problems", problems.count)]
      : problems.rolling.length === 0
        ? [t("home.state.fine", services, adopted.length)]
        : []
  if (problems.rolling.length > 0) parts.push(t("home.state.rolling", problems.rolling.length))
  const health = problems.count > 0 ? "fail" : problems.rolling.length > 0 ? "progress" : "ok"
  return (
    <section aria-label={t("home.status")} className="flex flex-wrap items-center gap-x-4 gap-y-1">
      <p className="flex items-center gap-2 font-medium">
        <StatusDot health={health} />
        {parts.join(" ")}
      </p>
      <ClusterFacts />
    </section>
  )
}

function ClusterFacts() {
  const t = useT()
  const cluster = useQuery(clusterQuery)
  const nodes = useQuery(nodesQuery)
  const upgrades = useQuery(upgradesQuery)
  const badge = useUpgradeBadge(upgrades.data)
  const version = cluster.data?.kubernetesVersion.split("+")[0]
  const items = nodes.data?.items
  const ready = items?.filter((node) => node.ready === "True").length
  if (!version && !items && !badge) return null
  return (
    <span className="flex flex-wrap items-center gap-2 type-meta text-muted-foreground">
      <Link
        to="/cluster"
        className="type-mono hover:text-foreground hover:underline"
        translate="no"
      >
        {version
          ? `${cluster.data?.distribution ? `${cluster.data.distribution} ` : ""}${version}`
          : t("shell.cluster")}
      </Link>
      {items && items.length > 0 ? (
        <span>{t("home.nodesReady", ready ?? 0, items.length)}</span>
      ) : null}
      {badge ? (
        <StatusPill health={badge.health} size="sm">
          {badge.text}
        </StatusPill>
      ) : null}
    </span>
  )
}

function ProblemsSection({ problems }: { problems: Problems }) {
  const t = useT()
  if (problems.count === 0 && problems.rolling.length === 0)
    return problems.notices > 0 ? <Notices count={problems.notices} /> : null
  return (
    <section aria-labelledby="home-problems" className="flex flex-col gap-3">
      <h2 id="home-problems" className="type-heading">
        {t("home.problems")}
      </h2>
      {problems.incidents.length > 0 ? (
        <Group
          title={t("incident.title")}
          hint={t("incident.hint")}
          count={problems.incidents.length}
        >
          {problems.incidents.map((incident) => (
            <IncidentRow key={incident.id} incident={incident} />
          ))}
        </Group>
      ) : null}
      {problems.byOrigin.map(({ origin, items }) => (
        <Group
          key={origin}
          title={t(`origin.${origin}`)}
          hint={t(`origin.${origin}.hint`)}
          count={items.length}
        >
          {items.map((f) => (
            <ProblemRow key={f.service.id} project={f.project} service={f.service} showProject />
          ))}
        </Group>
      ))}
      {problems.notices > 0 ? <Notices count={problems.notices} /> : null}
      {problems.rolling.length > 0 ? (
        <Group title={t("home.inProgress")} count={problems.rolling.length}>
          {problems.rolling.map((f) => (
            <ProblemRow key={f.service.id} project={f.project} service={f.service} showProject />
          ))}
        </Group>
      ) : null}
    </section>
  )
}

function Notices({ count }: { count: number }) {
  const t = useT()
  return (
    <p className="flex items-center gap-2 type-meta text-muted-foreground">
      <StatusDot health="warn" />
      {t("home.clusterNotices", count)}
      <Link to="/cluster" className="text-foreground hover:underline">
        {t("shell.cluster")}
      </Link>
    </p>
  )
}

function Group({
  title,
  hint,
  count,
  children,
}: {
  title: string
  hint?: string
  count?: number
  children: ReactNode
}) {
  return (
    <Card variant="list">
      <CardHeader className="flex items-center">
        <CardTitle>
          <span className="flex flex-wrap items-baseline gap-x-2.5">
            <h3 className="font-semibold">{title}</h3>
            {count !== undefined ? (
              <span className="type-meta font-normal text-muted-foreground">{count}</span>
            ) : null}
            {hint ? (
              <span className="type-meta font-normal text-muted-foreground">{hint}</span>
            ) : null}
          </span>
        </CardTitle>
      </CardHeader>
      <CardContent>
        <ul className="flex flex-col">{children}</ul>
      </CardContent>
    </Card>
  )
}

function byProjectHealth(a: ProjectItem, b: ProjectItem): number {
  const rank = (p: ProjectItem) => (p.health ? healthRank[countsHealth(p.health)] : healthRank.ok)
  return rank(a) - rank(b) || a.name.localeCompare(b.name)
}

function ProjectsSection({ adopted, error }: { adopted: ProjectItem[]; error: Error | null }) {
  const t = useT()
  const shown = adopted.toSorted(byProjectHealth).slice(0, PROJECTS_SHOWN)
  return (
    <section aria-labelledby="home-projects" className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-x-2.5 gap-y-2">
        <h2 id="home-projects" className="type-heading">
          {t("shell.projects")}
        </h2>
        {error ? null : <span className="type-meta text-muted-foreground">{adopted.length}</span>}
        <div className="ml-auto flex items-center gap-3">
          <Link to="/projects" className="type-meta text-secondary hover:text-main">
            {t("home.allProjects")}
          </Link>
          <ProjectViewSwitch />
        </div>
      </div>
      {error ? (
        isStatus(error, 403) ? (
          <Forbidden
            error={error}
            what={t("cluster.whatProjects")}
            missing={[{ group: "", resources: ["namespaces"] }]}
            role={overviewRole}
          />
        ) : (
          <Problem error={error} />
        )
      ) : shown.length === 0 ? (
        <Empty size="sm">
          <EmptyHeader>
            <EmptyMedia>
              <Illustration kind="empty" seed="projects" />
            </EmptyMedia>
            <EmptyTitle>{t("cluster.noProjects")}</EmptyTitle>
            <EmptyDescription>{t("cluster.noProjectsBody")}</EmptyDescription>
          </EmptyHeader>
          <EmptyContent>
            <Link to="/projects" className={buttonVariants({ variant: "outline", size: "sm" })}>
              {t("cluster.openProjects")}
            </Link>
          </EmptyContent>
        </Empty>
      ) : (
        <ProjectCollection projects={shown} label={t("shell.projects")} />
      )}
    </section>
  )
}

interface Folded {
  key: string
  namespace?: string
  event: Event
  count: number
}

/** A pod's warnings stay under the pod: the events read does not say which workload owns it. */
function fold(events: Event[]): Folded[] {
  const folded = new Map<string, Folded>()
  for (const event of events) {
    const namespace = event.origin.namespace ?? event.regarding?.namespace ?? undefined
    const object = event.regarding ? `${event.regarding.kind}/${event.regarding.name}` : ""
    const key = `${namespace ?? ""}/${object}/${event.reason}`
    const seen = folded.get(key)
    if (seen) seen.count += event.count
    else folded.set(key, { key, namespace, event, count: event.count })
  }
  return [...folded.values()]
}

const workloadKinds = new Set(["Deployment", "StatefulSet", "DaemonSet", "Job", "CronJob"])

function WarningsSection({ adopted }: { adopted: ProjectItem[] }) {
  const t = useT()
  const warnings = useQuery(recentWarningsQuery)
  if (warnings.isError && isStatus(warnings.error, 403)) return null
  const rows = warnings.data ? fold(warnings.data.items).slice(0, WARNINGS_SHOWN) : []
  const services = new Map(
    adopted.flatMap((p) => (p.services ?? []).map((s) => [`${p.name}/${s.kind}/${s.name}`, s])),
  )
  return (
    <section aria-labelledby="home-warnings" className="flex flex-col gap-2">
      <h2 id="home-warnings" className="type-heading">
        {t("cluster.warnings")}
      </h2>
      {warnings.isPending ? (
        <LoadingRows label={t("common.loading")} rows={2} />
      ) : warnings.isError ? (
        <Problem error={warnings.error} />
      ) : rows.length === 0 ? (
        <p className="flex items-center gap-2 text-muted-foreground">
          <StatusDot health="ok" />
          {t("cluster.noWarnings")}
        </p>
      ) : (
        <Collapsible>
          <CollapsibleTrigger variant="disclosure">
            <CaretRightIcon aria-hidden="true" />
            {t("home.showWarnings", rows.length)}
          </CollapsibleTrigger>
          <CollapsibleContent>
            <ul className="mt-2 flex flex-col">
              {rows.map((row) => {
                const regarding = row.event.regarding
                const service =
                  row.namespace && regarding && workloadKinds.has(regarding.kind)
                    ? services.get(`${row.namespace}/${regarding.kind}/${regarding.name}`)
                    : undefined
                return (
                  <li
                    key={row.key}
                    className="flex flex-wrap items-baseline gap-x-4 gap-y-0.5 border-t border-rule py-2.5 md:grid md:grid-cols-warning"
                  >
                    <span className="flex min-w-0 items-center gap-1.5">
                      <StatusDot health="warn" className="shrink-0" />
                      {row.namespace ? (
                        <>
                          <Link
                            to="/projects/$project"
                            params={{ project: row.namespace }}
                            search={{ events: "all" }}
                            className="max-w-1/2 min-w-0 truncate type-meta text-muted-foreground hover:text-foreground hover:underline"
                            translate="no"
                            title={row.namespace}
                          >
                            {row.namespace}
                          </Link>
                          <span
                            aria-hidden="true"
                            className="shrink-0 type-meta text-muted-foreground"
                          >
                            /
                          </span>
                        </>
                      ) : null}
                      {service && row.namespace ? (
                        <Link
                          {...serviceLink(row.namespace, service, { tab: "events" })}
                          className="min-w-0 truncate font-medium hover:underline"
                          translate="no"
                          title={regarding?.name}
                        >
                          {regarding?.name}
                        </Link>
                      ) : (
                        <span
                          className="min-w-0 truncate font-medium"
                          translate="no"
                          title={regarding?.name}
                        >
                          {regarding?.name}
                        </span>
                      )}
                      {regarding ? (
                        <span
                          className="shrink-0 whitespace-nowrap type-meta text-muted-foreground"
                          translate="no"
                        >
                          {regarding.kind}
                        </span>
                      ) : null}
                    </span>
                    <span className="min-w-0 truncate" title={row.event.note}>
                      <span className="font-semibold" translate="no">
                        {row.event.reason}
                      </span>
                      {row.count > 1 ? (
                        <span className="type-meta text-muted-foreground"> ×{row.count}</span>
                      ) : null}
                      {row.event.note ? (
                        <span className="text-muted-foreground"> {row.event.note}</span>
                      ) : null}
                    </span>
                    <Ago iso={row.event.lastSeen} className="type-meta text-muted-foreground" />
                  </li>
                )
              })}
            </ul>
          </CollapsibleContent>
        </Collapsible>
      )}
    </section>
  )
}
