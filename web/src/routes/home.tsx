import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import TerminalIcon from "~icons/ph/terminal"

import { isStatus, type Schemas } from "@/api/client"
import {
  clusterQuery,
  nodesQuery,
  projectQuery,
  projectsQuery,
  recentWarningsQuery,
  upgradesQuery,
} from "@/api/queries"
import { Ago } from "@/components/bits"
import { DiagnosisLine } from "@/components/diagnoses"
import { LoadingRows } from "@/components/page"
import { Problem } from "@/components/problem"
import { FactsPill, plainReason, StatusDot, StatusPill } from "@/components/status"
import { buttonVariants } from "@zeabur/design-system/components/button"
import { Card, CardContent, CardHeader, CardTitle } from "@zeabur/design-system/components/card"
import { Skeleton } from "@zeabur/design-system/components/skeleton"
import { useT, type Translate } from "@/i18n"
import { serviceLink } from "@/lib/place"
import { cn } from "@/lib/utils"
import { countsHealth, healthRank, type Event } from "@/model/service"
import { ProjectCollection, ProjectViewSwitch } from "@/components/project-card"
import { Forbidden, overviewRole } from "@/routes/cluster/forbidden"
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
import { Stat, StatGroup } from "@zeabur/design-system/components/stat"

type ProjectItem = Schemas["ProjectSummary"]
type Brief = Schemas["ServiceSummary"]

const PROJECTS_SHOWN = 9
const WARNINGS_SHOWN = 5

export function HomePage() {
  const t = useT()
  const projects = useQuery(projectsQuery)
  const adopted = projects.data?.items.filter((p) => p.adopted) ?? []
  const attention = needingAttention(adopted)
  return (
    <div className="mx-auto flex w-full max-w-6xl flex-col gap-8 px-4 pt-6 pb-12 type-body md:box-content md:px-8 md:pt-10">
      <PageHeader title={t("home.title")} />
      <StatusLine
        adopted={adopted}
        loaded={projects.isSuccess}
        allHealthy={projects.isSuccess && attention.length === 0}
      />
      <Attention items={attention} />
      <ProjectsSection
        adopted={adopted}
        pending={projects.isPending}
        error={projects.isError ? projects.error : null}
      />
      <WarningsSection adopted={adopted} />
    </div>
  )
}

function StatusLine({
  adopted,
  loaded,
  allHealthy,
}: {
  adopted: ProjectItem[]
  loaded: boolean
  allHealthy: boolean
}) {
  const t = useT()
  const totals = { services: 0, healthy: 0, failing: 0, warning: 0, progressing: 0 }
  for (const project of adopted) {
    const h = project.health
    if (!h) continue
    totals.services += h.services
    totals.healthy += h.healthy
    totals.failing += h.failing
    totals.warning += h.warning
    totals.progressing += h.progressing
  }
  const figure = (value: number | string) =>
    loaded ? value : <Skeleton className="my-1 h-4 w-10" />
  return (
    <section aria-label={t("home.status")}>
      <StatGroup>
        <Stat
          label={t("metric.services")}
          value={figure(`${totals.healthy}/${totals.services}`)}
          tone={loaded && totals.services > 0 && allHealthy ? "ok" : undefined}
        />
        <Stat
          label={t("status.failing")}
          value={figure(totals.failing)}
          tone={totals.failing > 0 ? "fail" : undefined}
        />
        <Stat
          label={t("status.degraded")}
          value={figure(totals.warning)}
          tone={totals.warning > 0 ? "warn" : undefined}
        />
        <Stat
          label={t("status.rollingOut")}
          value={figure(totals.progressing)}
          tone={totals.progressing > 0 ? "progress" : undefined}
        />
        <ClusterFacts />
      </StatGroup>
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
    <Stat
      label={t("shell.cluster")}
      value={
        <Link
          to="/cluster"
          className="rounded-md type-mono outline-none hover:underline"
          translate="no"
        >
          {version
            ? `${cluster.data?.distribution ? `${cluster.data.distribution} ` : ""}${version}`
            : t("shell.cluster")}
        </Link>
      }
      hint={
        (items && items.length > 0) || badge ? (
          <span className="flex items-center gap-2">
            {items && items.length > 0 ? t("home.nodesReady", ready ?? 0, items.length) : null}
            {badge ? (
              <StatusPill health={badge.health} size="sm">
                {badge.text}
              </StatusPill>
            ) : null}
          </span>
        ) : undefined
      }
    />
  )
}

interface AttentionItem {
  project: string
  service: Brief
}

function needingAttention(adopted: ProjectItem[]): AttentionItem[] {
  return adopted
    .flatMap((p) =>
      (p.services ?? [])
        .filter((s) => s.health === "fail" || s.health === "warn" || s.health === "progress")
        .map((service) => ({ project: p.name, service })),
    )
    .toSorted(
      (a, b) =>
        healthRank[a.service.health] - healthRank[b.service.health] ||
        a.project.localeCompare(b.project) ||
        a.service.name.localeCompare(b.service.name),
    )
}

function briefWord(t: Translate, service: Brief): string {
  const plain = plainReason(
    t,
    service.reason ?? undefined,
    service.kind === "Job" || service.kind === "CronJob",
  )
  if (plain) return plain
  switch (service.health) {
    case "fail":
      return t("status.failing")
    case "warn":
      return t("status.degraded")
    case "progress":
      return t("status.rollingOut")
    case "ok":
      return t("status.ready")
    case "idle":
      return t("status.unknown")
  }
}

function Attention({ items }: { items: AttentionItem[] }) {
  const t = useT()
  if (items.length === 0) return null
  return (
    <section aria-labelledby="home-attention">
      <Card variant="list">
        <CardHeader className="flex items-center">
          <CardTitle className="flex items-baseline">
            <h2 id="home-attention" className="type-heading">
              {t("attention.title")}
            </h2>
            <span className="ml-2.5 type-meta font-normal text-muted-foreground">
              {items.length}
            </span>
          </CardTitle>
        </CardHeader>
        <CardContent>
          <ul className="flex flex-col">
            {items.map((item) => (
              <AttentionRow key={item.service.id} item={item} />
            ))}
          </ul>
        </CardContent>
      </Card>
    </section>
  )
}

function AttentionRow({ item }: { item: AttentionItem }) {
  const t = useT()
  const { project, service } = item
  const facts = [
    service.desired !== undefined && service.desired !== null
      ? t("attention.ready", service.ready ?? 0, service.desired)
      : "",
    service.restarts > 0 ? t("pod.restarts", service.restarts) : "",
  ].filter(Boolean)
  return (
    <li className="flex flex-wrap items-center gap-x-4 gap-y-2 border-t border-rule px-4 py-3 first:border-t-0 md:grid md:grid-cols-attention">
      <span className="flex min-w-0 items-baseline gap-1.5">
        <Link
          to="/projects/$project"
          params={{ project }}
          className="type-meta text-muted-foreground hover:text-foreground hover:underline"
          translate="no"
        >
          {project}
        </Link>
        <span aria-hidden="true" className="type-meta text-muted-foreground">
          /
        </span>
        <Link
          {...serviceLink(project, service)}
          className="truncate font-semibold hover:underline"
          translate="no"
        >
          {service.name}
        </Link>
        <span className="type-meta text-muted-foreground" translate="no">
          {service.kind}
        </span>
      </span>
      <span className="flex">
        <FactsPill health={service.health} size="sm" facts={service.reason ?? undefined}>
          {briefWord(t, service)}
        </FactsPill>
      </span>
      <span className="flex min-w-0 flex-col">
        <span className="type-meta text-muted-foreground">{facts.join(t("common.sep"))}</span>
        <AttentionDiagnosis project={project} service={service} />
      </span>
      <span className="flex items-center gap-1.5 md:justify-end">
        <Link
          {...serviceLink(project, service, { tab: "logs" })}
          className={cn(buttonVariants({ variant: "outline", size: "sm" }))}
          aria-label={t("attention.logsOf", service.name)}
        >
          <TerminalIcon aria-hidden="true" data-icon="inline-start" />
          {t("tab.logs")}
        </Link>
        <Link
          {...serviceLink(project, service, { tab: "events" })}
          className={cn(buttonVariants({ variant: "outline", size: "sm" }))}
          aria-label={t("attention.eventsOf", service.name)}
        >
          {t("tab.events")}
        </Link>
      </span>
    </li>
  )
}

/** The project list carries no diagnoses, so they come from the project's document. */
function AttentionDiagnosis({ project, service }: { project: string; service: Brief }) {
  const document = useQuery(projectQuery(project, { poll: false }))
  const diagnoses = document.data?.services.find(
    (s) => s.kind === service.kind && s.name === service.name,
  )?.diagnoses
  return <DiagnosisLine diagnoses={diagnoses} />
}

function byProjectHealth(a: ProjectItem, b: ProjectItem): number {
  const rank = (p: ProjectItem) => (p.health ? healthRank[countsHealth(p.health)] : healthRank.ok)
  return rank(a) - rank(b) || a.name.localeCompare(b.name)
}

function ProjectsSection({
  adopted,
  pending,
  error,
}: {
  adopted: ProjectItem[]
  pending: boolean
  error: Error | null
}) {
  const t = useT()
  const shown = adopted.toSorted(byProjectHealth).slice(0, PROJECTS_SHOWN)
  return (
    <section aria-labelledby="home-projects" className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-x-2.5 gap-y-2">
        <h2 id="home-projects" className="type-heading">
          {t("shell.projects")}
        </h2>
        {pending || error ? null : (
          <span className="type-meta text-muted-foreground">{adopted.length}</span>
        )}
        <div className="ml-auto flex items-center gap-3">
          <Link to="/projects" className="type-meta text-secondary hover:text-main">
            {t("home.allProjects")}
          </Link>
          <ProjectViewSwitch />
        </div>
      </div>
      {pending ? (
        <LoadingRows label={t("common.loading")} rows={3} />
      ) : error ? (
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
        <ul className="flex flex-col">
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
                      <span aria-hidden="true" className="shrink-0 type-meta text-muted-foreground">
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
      )}
    </section>
  )
}
