import { useQueries, useQuery } from "@tanstack/react-query"
import { getRouteApi, Link, useNavigate } from "@tanstack/react-router"
import { useEffect, type ReactNode } from "react"
import CaretRightIcon from "~icons/ph/caret-right"

import { isStatus, isUnreachable, type Schemas } from "@/api/client"
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
import { IncidentGroupRow, isTakenDown, takenDown } from "@/components/incident"
import { groupIncidents, type IncidentGroup } from "@/model/incidents"
import { ProblemRow } from "@/components/problem-row"
import { StatusDot, StatusPill } from "@/components/status"
import { UnreachableBanner, useReachability } from "@/components/unreachable"
import { Card, CardContent, CardHeader, CardTitle } from "@zeabur-oss/design-system/components/card"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur-oss/design-system/components/collapsible"
import { Skeleton } from "@zeabur-oss/design-system/components/skeleton"
import { useT } from "@/i18n"
import { kindName } from "@/i18n/kind-name"
import { lastKnown, rememberKnown } from "@/lib/last-known"
import { serviceLink } from "@/lib/place"
import { originOf, originRank, type Origin } from "@/model/origin"
import { otherNamespaces } from "@/model/project"
import {
  byHealth,
  countsHealth,
  healthRank,
  needsAttention,
  statusOf,
  type Event,
  type Health,
  type Service,
} from "@/model/service"
import { severity } from "@/model/severity"
import { ProjectCollection, ProjectViewSwitch } from "@/components/project-card"
import { AttentionRow, useAttention, type ClusterItem } from "@/routes/cluster/attention"
import { Forbidden, overviewRole } from "@/routes/cluster/forbidden"
import { useUpgradeBadge } from "@/routes/cluster/upgrade"
import { OtherNamespaces } from "@/routes/projects"
import { PageHeader } from "@zeabur-oss/design-system/components/page-header"
import { PageColumn } from "@zeabur-oss/design-system/components/page-column"

type ProjectItem = Schemas.ProjectSummary

const homeRoute = getRouteApi("/")

const PROJECTS_SHOWN = 9
const WARNINGS_SHOWN = 5
/** Namespaces offered for adoption on an empty Home. */
const OFFERED = 5

export function HomePage() {
  const t = useT()
  const reach = useReachability()
  const projects = useQuery(projectsQuery)
  const adopted = projects.data?.items.filter((p) => p.adopted) ?? []
  const problems = useProblems(adopted)
  const shown = adopted.toSorted(byProjectHealth).slice(0, PROJECTS_SHOWN)
  useEffect(() => {
    if (projects.data)
      rememberKnown({
        projects: projects.data.items
          .filter((p) => p.adopted)
          .toSorted(byProjectHealth)
          .slice(0, PROJECTS_SHOWN),
      })
  }, [projects.data])
  if (reach.down)
    return (
      <PageColumn gap="lg" className="mt-6 mb-12 md:mt-10">
        <PageHeader title={t("home.title")} />
        <UnreachableBanner />
        <LastKnown />
      </PageColumn>
    )
  const empty = projects.isSuccess && adopted.length === 0
  return (
    <PageColumn gap="lg" className="mt-6 mb-12 md:mt-10">
      <div className="flex flex-col gap-2">
        <PageHeader title={t("home.title")} />
        <StateLine
          adopted={adopted}
          pending={projects.isPending || problems.pending}
          failed={projects.isError}
          problems={problems}
        />
      </div>
      {/* Problems come in after the projects; showing both at once keeps them from pushing the
          projects down. */}
      {projects.isPending || problems.pending ? (
        <LoadingRows label={t("common.loading")} rows={4} />
      ) : (
        <>
          <ProblemsSection problems={problems} />
          {empty ? (
            <Offered namespaces={otherNamespaces(projects.data?.items ?? [], false)} />
          ) : (
            <ProjectsSection
              total={adopted.length}
              shown={shown}
              error={projects.isError ? projects.error : null}
            />
          )}
          <WarningsSection adopted={adopted} />
        </>
      )}
    </PageColumn>
  )
}

/** While the cluster does not answer: the projects as this browser last saw them, greyed. */
function LastKnown() {
  const t = useT()
  const search = homeRoute.useSearch()
  const navigate = useNavigate({ from: "/" })
  const known = lastKnown()
  const projects = known.projects ?? []
  if (projects.length === 0) return null
  return (
    <section aria-labelledby="home-projects" className="flex flex-col gap-3">
      <div className="flex flex-wrap items-baseline gap-x-2.5 gap-y-1">
        <h2 id="home-projects" className="type-heading">
          {t("shell.projects")}
        </h2>
        <span className="type-meta text-muted-foreground">
          {t("home.lastKnown")}
          {known.at ? (
            <>
              {t("common.sep")}
              <Ago iso={new Date(known.at).toISOString()} />
            </>
          ) : null}
        </span>
      </div>
      <div className="opacity-60 grayscale">
        <ProjectCollection
          projects={projects}
          label={t("shell.projects")}
          offline
          search={search}
          onSearch={(patch) =>
            void navigate({ search: (previous) => ({ ...previous, ...patch }), replace: true })
          }
        />
      </div>
    </section>
  )
}

interface Found {
  project: string
  service: Service
}

interface Problems {
  /** Still reading the documents of the projects in trouble. */
  pending: boolean
  incidents: IncidentGroup[]
  /** Nodes down, full or under pressure, by outcome, that no incident above explains, and a
   * known vulnerability in Zeabur itself. */
  nodes: ClusterItem[]
  /** Cluster warnings that take nothing down yet: a count and a link. */
  notices: number
  byOrigin: { origin: Origin; items: Found[] }[]
  rolling: Found[]
  count: number
  health: Health
  /** Projects in trouble whose document could not be read: their problems are not listed. */
  unchecked: number
  /** The cluster-wide causes could not all be read. */
  incidentsUnchecked: boolean
}

/** Every problem once, by where it comes from: a cause in the cluster shows once, with the
 * services it takes down under it. */
function useProblems(adopted: ProjectItem[]): Problems {
  const attention = useAttention()
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
  const reported = new Set(
    incidents.filter((i) => i.kind === "nodeNotReady").map((i) => i.cause.params?.node),
  )
  const explained = (item: ClusterItem) =>
    item.nodes.length > 0 && item.nodes.every((node) => reported.has(node))
  const open = attention.items.filter(
    (i) => (i.health === "fail" || i.health === "warn") && !explained(i),
  )
  const nodes = open.filter((i) => i.problem || i.lift)
  const groups = groupIncidents(incidents)
  const listed = byOrigin.flatMap((g) => g.items)
  const health: Health =
    groups.length > 0 ||
    nodes.some((i) => i.health === "fail") ||
    listed.some((f) => statusOf(f.service).health === "fail")
      ? "fail"
      : "warn"
  return {
    pending: documents.some((d) => d.isPending) || incidentList.isPending || attention.pending,
    incidents: groups,
    nodes,
    notices: open.length - nodes.length,
    byOrigin,
    rolling: rolling.toSorted(byFound),
    count: groups.length + nodes.length + listed.length,
    health,
    unchecked: documents.filter((d) => d.isError).length,
    incidentsUnchecked:
      (incidentList.data?.unreadable.length ?? 0) > 0 ||
      (incidentList.isError && !isUnreachable(incidentList.error)),
  }
}

const byFound = (a: Found, b: Found) =>
  byHealth(a.service, b.service) ||
  a.project.localeCompare(b.project) ||
  a.service.name.localeCompare(b.service.name)

function StateLine({
  adopted,
  pending,
  failed,
  problems,
}: {
  adopted: ProjectItem[]
  pending: boolean
  failed: boolean
  problems: Problems
}) {
  const t = useT()
  if (pending) return <Skeleton className="h-5 w-72" />
  // The projects section says why the list could not be read.
  if (failed) return null
  const services = adopted.reduce((n, p) => n + (p.health?.services ?? 0), 0)
  const only = problems.nodes.length === 1 && problems.count === 1 ? problems.nodes[0] : undefined
  const parts: string[] = []
  let health: Health = "ok"
  if (problems.count > 0) {
    parts.push(only ? t("home.state.node", only.text) : t("home.state.problems", problems.count))
    health = problems.health
  } else if (problems.unchecked > 0) {
    parts.push(t("home.state.unchecked", problems.unchecked))
    health = "warn"
  } else if (adopted.length === 0) {
    parts.push(t("home.state.empty"))
    health = "idle"
  } else if (problems.rolling.length === 0)
    parts.push(t("home.state.fine", services, adopted.length))
  if (problems.rolling.length > 0) {
    parts.push(t("home.state.rolling", problems.rolling.length))
    if (health === "ok") health = "progress"
  }
  return (
    <section aria-label={t("home.status")} className="flex flex-wrap items-center gap-x-4 gap-y-1">
      <p className="flex items-center gap-2 font-medium">
        <StatusDot health={health} />
        {t("common.sentences", parts)}
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
        className="font-medium text-foreground hover:underline"
        translate={cluster.data ? "no" : undefined}
      >
        {cluster.data?.name ?? t("shell.cluster")}
      </Link>
      {version ? (
        <span className="type-mono" translate="no">
          {`${cluster.data?.distribution ? `${cluster.data.distribution} ` : ""}${version}`}
        </span>
      ) : null}
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
  const unchecked = <Unchecked problems={problems} />
  if (problems.count === 0 && problems.rolling.length === 0)
    return problems.notices > 0 || problems.unchecked > 0 || problems.incidentsUnchecked ? (
      <div className="flex flex-col gap-2">
        {unchecked}
        {problems.notices > 0 ? <Notices count={problems.notices} /> : null}
      </div>
    ) : null
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
          {problems.incidents.map((group) => (
            <IncidentGroupRow key={group.key} group={group} />
          ))}
        </Group>
      ) : null}
      {problems.nodes.length > 0 ? (
        <Group title={t("cluster.title")} count={problems.nodes.length}>
          {problems.nodes.map((item) => (
            <AttentionRow key={item.key} item={item} inset />
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
      {unchecked}
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

/** What Home could not check, so a quiet page is not read as a healthy one. */
function Unchecked({ problems }: { problems: Problems }) {
  const t = useT()
  if (problems.unchecked === 0 && !problems.incidentsUnchecked) return null
  return (
    <ul className="flex flex-col gap-1 type-meta text-muted-foreground">
      {problems.unchecked > 0 ? (
        <li className="flex items-center gap-2">
          <StatusDot health="idle" />
          {t("home.unchecked", problems.unchecked)}
          <Link to="/projects" className="text-foreground hover:underline">
            {t("shell.projects")}
          </Link>
        </li>
      ) : null}
      {problems.incidentsUnchecked ? (
        <li className="flex items-center gap-2">
          <StatusDot health="idle" />
          {t("home.incidentsUnchecked")}
        </li>
      ) : null}
    </ul>
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

/** No project yet: the namespaces that could be one, each with Adopt. */
function Offered({ namespaces }: { namespaces: ReturnType<typeof otherNamespaces> }) {
  const t = useT()
  return (
    <section aria-labelledby="home-offered" className="flex flex-col gap-3">
      <div className="flex flex-wrap items-baseline gap-x-2.5 gap-y-1">
        <h2 id="home-offered" className="type-heading">
          {t("home.adoptTitle")}
        </h2>
        <span className="type-meta text-muted-foreground">{t("cluster.noProjectsBody")}</span>
      </div>
      <OtherNamespaces namespaces={namespaces} limit={OFFERED} />
    </section>
  )
}

function ProjectsSection({
  total,
  shown,
  error,
}: {
  total: number
  shown: ProjectItem[]
  error: Error | null
}) {
  const t = useT()
  const search = homeRoute.useSearch()
  const navigate = useNavigate({ from: "/" })
  return (
    <section aria-labelledby="home-projects" className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-x-2.5 gap-y-2">
        <h2 id="home-projects" className="type-heading">
          {t("shell.projects")}
        </h2>
        {error ? null : <span className="type-meta text-muted-foreground">{total}</span>}
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
      ) : (
        <ProjectCollection
          projects={shown}
          label={t("shell.projects")}
          search={search}
          onSearch={(patch) =>
            void navigate({ search: (previous) => ({ ...previous, ...patch }), replace: true })
          }
        />
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
  const folded = warnings.data ? fold(warnings.data.items) : []
  const rows = folded.slice(0, WARNINGS_SHOWN)
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
            {folded.length > rows.length
              ? t("home.showWarningsOf", rows.length, folded.length)
              : t("home.showWarnings", rows.length)}
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
                      <StatusDot health={severity.warningEvent} className="shrink-0" />
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
                          {kindName(t, regarding.kind)}
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
            {folded.length > rows.length ? (
              <Link
                to="/cluster"
                hash="cluster-warnings"
                className="mt-1 inline-block type-meta text-secondary hover:text-main"
              >
                {t("home.allWarnings", folded.length)}
              </Link>
            ) : null}
          </CollapsibleContent>
        </Collapsible>
      )}
    </section>
  )
}
