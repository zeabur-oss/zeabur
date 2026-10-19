// Home: what is running and what is broken, across projects. A status line (services by health,
// the cluster's version, nodes and upgrade state), the services that need attention, the projects
// failing first, and the last hour's warnings. Everything comes from the projects list, which
// carries each adopted project's services with their health, and the cluster's own reads; each
// part leaves out what the person may not read rather than guessing it.
import { useQueries, useQuery } from "@tanstack/react-query"
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
import { buttonVariants } from "@engawahq/design-system/components/button"
import { Card, CardContent, CardHeader, CardTitle } from "@engawahq/design-system/components/card"
import { Skeleton } from "@engawahq/design-system/components/skeleton"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@engawahq/design-system/components/table"
import { Tooltip } from "@engawahq/design-system/components/tooltip"
import { useT, type Translate } from "@/i18n"
import { useSeen } from "@/lib/in-view"
import { serviceLink } from "@/lib/place"
import { cn } from "@/lib/utils"
import {
  countsHealth,
  domainsOf,
  healthRank,
  hostUrl,
  type Domain,
  type Event,
  type Health,
  type Project,
} from "@/model/service"
import { Forbidden, overviewRole } from "@/routes/cluster/forbidden"
import { useUpgradeBadge } from "@/routes/cluster/upgrade"
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
} from "@engawahq/design-system/components/empty"
import { Illustration } from "@engawahq/design-system/components/illustration"
import { PageHeader } from "@engawahq/design-system/components/page-header"
import { Stat, StatGroup } from "@engawahq/design-system/components/stat"

type ProjectItem = Schemas["ProjectSummary"]
type Brief = Schemas["ServiceSummary"]

/** Rows the Projects section shows before "All projects". */
const PROJECTS_SHOWN = 20
/** Folded warnings the page shows before "All events". */
const WARNINGS_SHOWN = 5

export function HomePage() {
  const t = useT()
  const projects = useQuery(projectsQuery)
  const adopted = projects.data?.items.filter((p) => p.adopted) ?? []
  const attention = needingAttention(adopted)
  return (
    <div className="flex w-full max-w-310 flex-col gap-8 px-4 pt-6 pb-12 type-body md:px-8 md:pt-7">
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

// ── Key figures ─────────────────────────────────────────────────────────────────────────────────

/** The services of every adopted project by state, then the cluster: a console's row of figures. */
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

/** The cluster: distribution and version (a link to it), nodes ready and the upgrade pill; each
 * part only when readable. */
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
      // Nodes ready and the upgrade pill, each when it is readable: one does not wait for the other.
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

// ── Needs attention ──────────────────────────────────────────────────────────────────────────────

interface AttentionItem {
  project: string
  service: Brief
}

/** Failing, then not ready, then rolling-out services of every project; then by project, name. */
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

/** A service's status word: what its failing pod's reason means, when known. */
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

/** The cause of a service's first diagnosis, from its project's document. The list on Home has
 * no events, so the document (one per project with something to attend to, shared with the
 * project cards) supplies it; until it arrives the row shows the facts alone. */
function AttentionDiagnosis({ project, service }: { project: string; service: Brief }) {
  const document = useQuery(projectQuery(project, { poll: false }))
  const diagnoses = document.data?.services.find(
    (s) => s.kind === service.kind && s.name === service.name,
  )?.diagnoses
  return <DiagnosisLine diagnoses={diagnoses} />
}

// ── Projects ─────────────────────────────────────────────────────────────────────────────────────

/** Failing projects first, then warning, rolling out and healthy; by name within each. */
function byProjectHealth(a: ProjectItem, b: ProjectItem): number {
  const rank = (p: ProjectItem) => (p.health ? healthRank[countsHealth(p.health)] : healthRank.ok)
  return rank(a) - rank(b) || a.name.localeCompare(b.name)
}

/** Warning events of the last hour per namespace, `null` when they may not be read, `undefined`
 * while loading; `partial` when there were more than one page holds. */
function useWarningCounts() {
  const warnings = useQuery(recentWarningsQuery)
  if (warnings.isError && isStatus(warnings.error, 403)) return { counts: null, partial: false }
  const data = warnings.data
  if (!data) return { counts: undefined, partial: false }
  const counts = new Map<string, number>()
  for (const event of data.items) {
    const namespace = event.origin.namespace ?? event.regarding?.namespace
    if (namespace) counts.set(namespace, (counts.get(namespace) ?? 0) + 1)
  }
  const partial = data.partial || data.totalSize > data.items.length
  return { counts, partial }
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
  const [ref, seen] = useSeen<HTMLElement>()
  const shown = adopted.toSorted(byProjectHealth).slice(0, PROJECTS_SHOWN)
  // The domains and the services' roles (for the outside-to-inside order) are in each project's
  // document: read the shown rows' documents once they are near the viewport, without polling.
  const documents = useQueries({
    queries: shown.map((p) => ({ ...projectQuery(p.name, { poll: false }), enabled: seen })),
  })
  const warnings = useWarningCounts()
  return (
    <section ref={ref} aria-labelledby="home-projects" className="flex flex-col gap-2">
      <div className="flex flex-wrap items-center gap-x-2.5 gap-y-2">
        <h2 id="home-projects" className="type-heading">
          {t("shell.projects")}
        </h2>
        {pending || error ? null : (
          <span className="type-meta text-muted-foreground">{adopted.length}</span>
        )}
        <Link
          to="/projects"
          className="ml-auto type-meta text-muted-foreground hover:text-foreground hover:underline"
        >
          {t("home.allProjects")}
        </Link>
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
            <EmptyDescription>{t("cluster.noProjects")}</EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : (
        <Table aria-labelledby="home-projects" variant="ruled" className="min-w-180">
          <TableHeader>
            <TableRow>
              <TableHead>{t("table.name")}</TableHead>
              <TableHead>{t("metric.services")}</TableHead>
              <TableHead>{t("home.warningsHour")}</TableHead>
              <TableHead>{t("table.domain")}</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {shown.map((project, i) => (
              <ProjectRow
                key={project.id}
                project={project}
                document={documents[i]?.data}
                documentPending={seen && (documents[i]?.isPending ?? false)}
                warnings={
                  warnings.counts === null
                    ? null
                    : warnings.counts === undefined
                      ? undefined
                      : (warnings.counts.get(project.name) ?? 0)
                }
                warningsPartial={warnings.partial}
              />
            ))}
          </TableBody>
        </Table>
      )}
    </section>
  )
}

/** The domain a project is known by: the one with the fewest labels (`shop.example.com` before
 * `api.shop.example.com`), then the shortest, then by name. */
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

function ProjectRow({
  project,
  document,
  documentPending,
  warnings,
  warningsPartial,
}: {
  project: ProjectItem
  document?: Project
  documentPending: boolean
  /** The last hour's warning events, `null` without permission, `undefined` while loading. */
  warnings: number | null | undefined
  /** More warnings than were read: the count is a lower bound. */
  warningsPartial: boolean
}) {
  const t = useT()
  const health: Health = project.health ? countsHealth(project.health) : "idle"
  const domain = document ? primaryDomain(document) : undefined
  return (
    <TableRow>
      <TableCell>
        <span className="flex items-center gap-2">
          <StatusDot health={health} />
          <Link
            to="/projects/$project"
            params={{ project: project.name }}
            className="font-semibold hover:underline"
            translate="no"
          >
            {project.displayName ?? project.name}
          </Link>
        </span>
      </TableCell>
      <TableCell>
        <ServicesCell project={project} />
      </TableCell>
      <TableCell>
        {warnings === null ? (
          <Tooltip content={t("home.noEventsPermission")}>
            <span className="text-muted-foreground" tabIndex={0}>
              —<span className="sr-only">{t("home.noEventsPermission")}</span>
            </span>
          </Tooltip>
        ) : warnings === undefined ? (
          <Skeleton className="h-4 w-6" />
        ) : warnings === 0 && !warningsPartial ? (
          <span className="text-muted-foreground">—</span>
        ) : (
          <span>
            {warnings}
            {warningsPartial ? "+" : ""}
          </span>
        )}
      </TableCell>
      <TableCell className="max-w-56">
        {domain ? (
          <a
            href={hostUrl(domain)}
            target="_blank"
            rel="noreferrer noopener"
            className="block truncate type-mono text-muted-foreground hover:text-foreground hover:underline"
            translate="no"
          >
            {domain.host}
            <span className="sr-only"> {t("common.newTab")}</span>
          </a>
        ) : documentPending ? (
          <Skeleton className="h-4 w-32" />
        ) : (
          <span className="text-muted-foreground">—</span>
        )}
      </TableCell>
    </TableRow>
  )
}

/** A project's services in words: healthy out of all, then what needs attention, worst first
 * (the replicas of each service are on the project's own pages). */
function ServicesCell({ project }: { project: ProjectItem }) {
  const t = useT()
  const h = project.health
  if (!project.services) return <span className="text-muted-foreground">—</span>
  if (project.services.length === 0 || !h || h.services === 0)
    return <span className="text-muted-foreground">{t("home.noServices")}</span>
  return (
    <span className="flex items-center gap-2.5">
      <span className="tabular-nums">
        {h.healthy}/{h.services}
      </span>
      {h.failing > 0 ? (
        <StatusPill health="fail" size="sm">
          {t("overview.failingCount", h.failing)}
        </StatusPill>
      ) : h.warning > 0 ? (
        <StatusPill health="warn" size="sm">
          {t("overview.degradedCount", h.warning)}
        </StatusPill>
      ) : h.progressing > 0 ? (
        <StatusPill health="progress" size="sm">
          {t("home.rollingOut", h.progressing)}
        </StatusPill>
      ) : null}
    </span>
  )
}

// ── Recent warnings ──────────────────────────────────────────────────────────────────────────────

interface Folded {
  key: string
  namespace?: string
  event: Event
  count: number
}

/** The last hour's warnings folded by namespace, the object they are about and reason, newest
 * first. A pod's warnings stay under the pod: the events read does not say which workload owns
 * it. */
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
  // A whole area the person may not read is left out.
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
