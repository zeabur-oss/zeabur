// Project › Overview: where it is broken first. The project's name and health, the pod strip (every
// service's pods, outside to inside) with the counts, then what is failing with direct links to the
// failing pod's logs and events, the services table (failures first, with live CPU and memory when
// metrics-server is there) and the events; domains and storage at the side.
import { useQuery } from "@tanstack/react-query"
import { getRouteApi, Link } from "@tanstack/react-router"
import type { ReactNode } from "react"
import ArrowRightIcon from "~icons/ph/arrow-right"
import LockSimpleIcon from "~icons/ph/lock-simple"
import LockSimpleOpenIcon from "~icons/ph/lock-simple-open"
import TerminalIcon from "~icons/ph/terminal"

import type { Schemas } from "@/api/client"
import { projectGapsQuery, projectQuery, usageQuery } from "@/api/queries"
import { Ago, Segmented } from "@/components/bits"
import { DiagnosisLine } from "@/components/diagnoses"
import { EventList } from "@/components/events"
import { GapList } from "@/components/gaps"
import { PodCells } from "@/components/pods"
import { refusedAmong, RefusedKinds } from "@/components/refused"
import { ServiceIcon } from "@/components/service-icon"
import { ServiceStatus, StatusDot, StatusPill } from "@/components/status"
import { buttonVariants } from "@zeabur/design-system/components/button"
import { Card, CardContent, CardHeader, CardTitle } from "@zeabur/design-system/components/card"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@zeabur/design-system/components/table"
import { Tooltip } from "@zeabur/design-system/components/tooltip"
import {
  BackupDetails,
  CoverageRow,
  ProviderNotes,
  shownCoverage,
  useBackups,
} from "@/components/backups"
import { useT } from "@/i18n"
import { podEventsLink, podLogsLink, serviceLink } from "@/lib/place"
import { useProjectSummary } from "@/lib/project-name"
import { useQuantityFormat } from "@/lib/quantity"
import { cn } from "@/lib/utils"
import { projectTitle } from "@/model/project"
import {
  byHealth,
  claimsOf,
  hostUrl,
  isWarning,
  needsAttention,
  restartsOf,
  serviceId,
  statusOf,
  summarise,
  worstPod,
  type Event,
  type Health,
  type Project,
  type Service,
} from "@/model/service"
import { serviceUsage } from "@/model/usage"
import { ProjectData, refusedPlaces, useProjectEvents } from "@/routes/project/data"
import { PageHeader } from "@zeabur/design-system/components/page-header"
import { Stat, StatGroup } from "@zeabur/design-system/components/stat"
import { MeterStack, meterPartTone } from "@zeabur/design-system/components/meter"

const route = getRouteApi("/projects/$project")

export function OverviewPage() {
  const { project } = route.useParams()
  return (
    <>
      <ProjectData project={project}>
        {(data, unreadable) => <Overview project={project} data={data} unreadable={unreadable} />}
      </ProjectData>
    </>
  )
}

function Overview({
  project,
  data,
  unreadable,
}: {
  project: string
  data: Project
  unreadable: Schemas["UnreadableView"][]
}) {
  const events = useProjectEvents(project)
  // A refused kind shows in the section its objects feed; Secrets, ConfigMaps, Services and
  // NetworkPolicies feed no list here, so they are not mentioned.
  const refused = (resources: readonly string[]) => refusedAmong(unreadable, resources)
  return (
    <div className="mx-auto flex w-full max-w-6xl flex-col gap-7 px-4 pt-6 pb-12 type-body md:px-8 md:pt-8">
      <Health project={project} data={data} />
      <div className="flex flex-wrap items-start gap-x-8 gap-y-7">
        <div className="flex min-w-0 flex-main flex-col gap-7">
          <Failing project={project} services={data.services} />
          <ServicesSummary
            project={project}
            services={data.services}
            refused={refused(refusedPlaces.services)}
          />
          <Gaps project={project} data={data} />
          <RecentEvents
            project={project}
            services={data.services}
            events={events}
            refused={refused(refusedPlaces.events)}
          />
        </div>
        <div className="flex min-w-0 flex-side flex-col gap-7">
          <Domains project={project} data={data} refused={refused(refusedPlaces.routes)} />
          <Storage project={project} data={data} refused={refused(refusedPlaces.claims)} />
          <Backups project={project} />
        </div>
      </div>
    </div>
  )
}

/** What the project is missing (limits, probes, Ingress to HTTPRoute), each with its fix. Read
 * on its own, not polled with the document; hidden when nothing is missing. */
function Gaps({ project, data }: { project: string; data: Project }) {
  const t = useT()
  const gaps = useQuery(projectGapsQuery(project))
  const items = gaps.data?.items ?? []
  if (items.length === 0) return null
  return (
    <section aria-labelledby="overview-gaps" className="flex flex-col gap-2">
      <Heading id="overview-gaps" title={t("gap.title")} count={items.length} />
      <GapList gaps={items} project={project} document={data} />
      {gaps.data && gaps.data.unreadable.length > 0 ? (
        <p className="type-meta text-muted-foreground">{t("gap.unreadable")}</p>
      ) : null}
    </section>
  )
}

/** A section heading with its count and an optional action at the right. */
function Heading({
  id,
  title,
  count,
  children,
  className,
}: {
  id: string
  title: ReactNode
  count?: ReactNode
  children?: ReactNode
  className?: string
}) {
  return (
    <div className={cn("flex flex-wrap items-center gap-x-2.5 gap-y-2", className)}>
      <h2 id={id} className="type-heading">
        {title}
      </h2>
      {count !== undefined ? (
        <span className="type-meta text-muted-foreground">{count}</span>
      ) : null}
      {children ? <div className="ml-auto flex items-center">{children}</div> : null}
    </div>
  )
}

// ── Health: title, pill, pod strip, counts ───────────────────────────────────────────────────────

function Health({ project, data }: { project: string; data: Project }) {
  const t = useT()
  const view = useQuery(projectQuery(project))
  const details = useProjectSummary(project)
  const summary = summarise(data)
  const statuses = data.services.map(statusOf)
  const count = (health: Health) => statuses.filter((s) => s.health === health).length
  let pill: ReactNode = null
  if (summary.services > 0) {
    if (summary.health === "fail")
      pill = <StatusPill health="fail">{t("overview.failingCount", count("fail"))}</StatusPill>
    else if (summary.health === "warn")
      pill = <StatusPill health="warn">{t("overview.degradedCount", count("warn"))}</StatusPill>
    else if (summary.health === "progress")
      pill = <StatusPill health="progress">{t("status.rollingOut")}</StatusPill>
    else pill = <StatusPill health="ok">{t("overview.healthy")}</StatusPill>
  }
  // A console's overview: the header and the key figures from the left under it; each service's
  // replicas are in the services table below.
  return (
    <section aria-label={t("overview.health")} className="flex flex-col gap-5">
      <PageHeader
        title={<span translate="no">{projectTitle(details, project)}</span>}
        badges={
          <>
            {details?.displayName ? (
              <span className="type-meta text-muted-foreground" translate="no">
                {project}
              </span>
            ) : null}
            {pill}
          </>
        }
        actions={
          view.dataUpdatedAt ? (
            <span className="type-meta text-muted-foreground">
              {t("overview.updated")} <Ago iso={new Date(view.dataUpdatedAt).toISOString()} />
            </span>
          ) : undefined
        }
        description={
          details?.description ? (
            <span className="whitespace-pre-line">{details.description}</span>
          ) : undefined
        }
      />
      {summary.services > 0 ? (
        <>
          <StatGroup>
            <Stat
              label={t("metric.services")}
              value={`${summary.healthy}/${summary.services}`}
              tone={summary.health}
            />
            <Stat label={t("metric.podsReady")} value={`${summary.podsReady}/${summary.pods}`} />
            <Stat label={t("metric.restarts")} value={summary.restarts} />
          </StatGroup>
        </>
      ) : (
        <p className="text-muted-foreground">{t("project.noServices")}</p>
      )}
    </section>
  )
}

// ── Failing ──────────────────────────────────────────────────────────────────────────────────────

function Failing({ project, services }: { project: string; services: Service[] }) {
  const t = useT()
  const broken = services.filter((s) => needsAttention(statusOf(s))).toSorted(byHealth)
  if (broken.length === 0) {
    if (services.length === 0) return null
    return (
      <p className="flex items-center gap-2 text-muted-foreground">
        <StatusDot health="ok" />
        {t("attention.none")}
      </p>
    )
  }
  const failing = broken.some((s) => statusOf(s).health === "fail")
  return (
    <section aria-labelledby="overview-failing">
      <Card variant="list">
        <CardHeader className="flex items-center">
          <CardTitle className="flex items-baseline">
            <h2 id="overview-failing" className="type-heading">
              {failing ? t("status.failing") : t("status.degraded")}
            </h2>
            <span className="ml-2.5 type-meta font-normal text-muted-foreground">
              {broken.length}
            </span>
          </CardTitle>
        </CardHeader>
        <CardContent>
          <ul className="flex flex-col">
            {broken.map((service) => (
              <FailingRow key={serviceId(service)} project={project} service={service} />
            ))}
          </ul>
        </CardContent>
      </Card>
    </section>
  )
}

function FailingRow({ project, service }: { project: string; service: Service }) {
  const t = useT()
  const status = statusOf(service)
  const pod = worstPod(service)
  const facts = [
    status.desired !== undefined ? t("attention.ready", status.ready ?? 0, status.desired) : "",
    t("pod.restarts", restartsOf(service)),
  ].filter(Boolean)
  return (
    <li className="flex flex-wrap items-center gap-x-4 gap-y-3 border-t border-rule px-4 py-3.5 first:border-t-0">
      <ServiceIcon service={service} />
      <div className="flex min-w-64 flex-1 flex-col gap-1">
        <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1">
          <Link
            {...serviceLink(project, service)}
            className="font-semibold hover:underline"
            translate="no"
          >
            {service.name}
          </Link>
          <ServiceStatus status={status} size="sm" />
          {facts.map((fact) => (
            <span key={fact} className="type-meta text-muted-foreground">
              {fact}
            </span>
          ))}
        </div>
        <DiagnosisLine diagnoses={service.diagnoses} />
        {pod ? (
          <div className="type-meta text-muted-foreground">
            {t("events.forPod")}{" "}
            <Link
              {...podLogsLink(project, service, pod.name)}
              className="type-mono text-foreground hover:underline"
              translate="no"
            >
              {pod.name}
            </Link>
          </div>
        ) : null}
      </div>
      <div className="flex shrink-0 items-center gap-1.5">
        {pod ? (
          <>
            <Link
              {...podLogsLink(project, service, pod.name)}
              className={cn(buttonVariants({ variant: "outline", size: "sm" }))}
              aria-label={t("attention.logsOf", pod.name)}
            >
              <TerminalIcon aria-hidden="true" data-icon="inline-start" />
              {t("tab.logs")}
            </Link>
            <Link
              {...podEventsLink(project, service, pod.name)}
              className={cn(buttonVariants({ variant: "outline", size: "sm" }))}
              aria-label={t("attention.eventsOf", pod.name)}
            >
              {t("tab.events")}
            </Link>
          </>
        ) : (
          <Link
            {...serviceLink(project, service, { tab: "events" })}
            className={cn(buttonVariants({ variant: "outline", size: "sm" }))}
          >
            {t("tab.events")}
          </Link>
        )}
      </div>
    </li>
  )
}

// ── Services ─────────────────────────────────────────────────────────────────────────────────────

function ServicesSummary({
  project,
  services,
  refused,
}: {
  project: string
  services: Service[]
  refused: Schemas["UnreadableView"][]
}) {
  const t = useT()
  const f = useQuantityFormat()
  const usage = useQuery(usageQuery(project)).data
  // Columns only with numbers to put in them: no zeros where metrics-server is missing.
  const live = usage?.state === "available"
  if (services.length === 0 && refused.length === 0) return null
  return (
    <section aria-labelledby="overview-services" className="flex flex-col gap-2">
      <Heading id="overview-services" title={t("project.services")} count={services.length}>
        <Link
          to="/projects/$project/services"
          params={{ project }}
          className="type-meta text-muted-foreground hover:text-foreground hover:underline"
        >
          {t("overview.viewAll")}
        </Link>
      </Heading>
      <RefusedKinds unreadable={refused} />
      <div className="overflow-hidden rounded-lg border">
        <Table aria-labelledby="overview-services" variant="ruled" className="min-w-135">
          <TableHeader>
            <TableRow>
              <TableHead>{t("table.name")}</TableHead>
              <TableHead>{t("table.status")}</TableHead>
              <TableHead>{t("table.pods")}</TableHead>
              <TableHead className="text-right">{t("table.restarts")}</TableHead>
              {live ? (
                <>
                  <TableHead className="text-right">{t("resources.cpu")}</TableHead>
                  <TableHead className="text-right">{t("resources.memory")}</TableHead>
                </>
              ) : null}
            </TableRow>
          </TableHeader>
          <TableBody>
            {services.toSorted(byHealth).map((service) => {
              const status = statusOf(service)
              const restarts = restartsOf(service)
              const used = serviceUsage(service, usage)
              return (
                <TableRow key={serviceId(service)}>
                  <TableCell>
                    <span className="flex min-w-0 items-center gap-2.5">
                      <Link
                        {...serviceLink(project, service)}
                        className="font-medium hover:underline"
                        translate="no"
                      >
                        {service.name}
                      </Link>
                      <span className="type-meta text-muted-foreground">{service.kind}</span>
                    </span>
                  </TableCell>
                  <TableCell>
                    <ServiceStatus status={status} size="sm" />
                  </TableCell>
                  <TableCell>
                    <PodCells service={service} project={project} />
                  </TableCell>
                  <TableCell className="text-right">
                    <span
                      className={cn(status.health === "fail" && restarts > 0 && "font-semibold")}
                    >
                      {restarts}
                    </span>
                  </TableCell>
                  {live ? (
                    <>
                      <TableCell className="text-right">
                        <span className="tabular-nums">
                          {used?.cpu !== undefined ? f.cpu(used.cpu) : "—"}
                        </span>
                      </TableCell>
                      <TableCell className="text-right">
                        <span
                          className={cn(
                            "tabular-nums",
                            used?.nearLimit && "font-semibold text-warn-strong",
                          )}
                        >
                          {used?.memory !== undefined ? f.memory(used.memory) : "—"}
                          {used?.nearLimit ? (
                            <span className="sr-only">{` ${t("usage.note.nearLimit")}`}</span>
                          ) : null}
                        </span>
                      </TableCell>
                    </>
                  ) : null}
                </TableRow>
              )
            })}
          </TableBody>
        </Table>
      </div>
    </section>
  )
}

// ── Events ───────────────────────────────────────────────────────────────────────────────────────

function RecentEvents({
  project,
  services,
  events,
  refused,
}: {
  project: string
  services: Service[]
  events: Event[]
  refused: Schemas["UnreadableView"][]
}) {
  const t = useT()
  const search = route.useSearch()
  const navigate = route.useNavigate()
  const all = search.events === "all"
  const warnings = events.filter(isWarning)
  const choose = (value: "warnings" | "all") =>
    void navigate({
      search: (previous) => ({ ...previous, events: value === "all" ? "all" : undefined }),
      replace: true,
    })

  /** The service an event is about, by its workload or one of its pods. */
  const objectLink = (event: Event, label: string) => {
    const regarding = event.regarding
    if (!regarding) return label
    const service = services.find((s) =>
      regarding.kind === "Pod"
        ? s.pods.some((p) => p.name === regarding.name)
        : s.kind === regarding.kind && s.name === regarding.name,
    )
    if (!service) return label
    const link =
      regarding.kind === "Pod"
        ? podEventsLink(project, service, regarding.name)
        : serviceLink(project, service, { tab: "events" })
    return (
      <Link {...link} className="hover:text-foreground hover:underline">
        {label}
      </Link>
    )
  }

  return (
    <section aria-labelledby="overview-events" className="flex flex-col gap-3">
      <Heading id="overview-events" title={t("project.events")}>
        <Segmented
          label={t("events.show")}
          value={all ? "all" : "warnings"}
          onChange={choose}
          options={[
            { value: "warnings", label: counted(t("events.warnings"), warnings.length) },
            { value: "all", label: counted(t("common.all"), events.length) },
          ]}
        />
      </Heading>
      <RefusedKinds unreadable={refused} />
      <EventList
        events={all ? events : warnings}
        limit={all ? undefined : 8}
        empty={all ? undefined : t("events.noWarnings")}
        objectLink={objectLink}
      />
    </section>
  )
}

/** A segment's label and its count, the count quieter. */
function counted(label: string, count: number) {
  return (
    <>
      {label}
      <span className="font-normal text-muted-foreground tabular-nums">{count}</span>
    </>
  )
}

// ── Side: domains and storage ────────────────────────────────────────────────────────────────────

const sideRow = "flex items-center gap-2.5 border-t border-rule py-2.75"

function Domains({
  project,
  data,
  refused,
}: {
  project: string
  data: Project
  refused: Schemas["UnreadableView"][]
}) {
  const t = useT()
  const domains = summarise(data).domains
  return (
    <section aria-labelledby="overview-domains" className="flex flex-col gap-1">
      <Heading id="overview-domains" title={t("common.domains")} count={domains.length} />
      <RefusedKinds unreadable={refused} line />
      {domains.length === 0 ? (
        refused.length > 0 ? null : (
          <p className="border-t border-rule py-2.75 text-muted-foreground">{t("domains.none")}</p>
        )
      ) : (
        <ul className="flex flex-col">
          {domains.map((domain) => {
            const Lock = domain.https ? LockSimpleIcon : LockSimpleOpenIcon
            return (
              <li key={domain.host} className={sideRow}>
                <Tooltip content={domain.https ? t("domains.https") : "HTTP"}>
                  <Lock
                    aria-hidden="true"
                    className={cn(
                      "size-3.5 shrink-0",
                      domain.https ? "text-ok" : "text-muted-foreground",
                    )}
                  />
                </Tooltip>
                <div className="flex min-w-0 flex-1 flex-col">
                  {domain.https ? <span className="sr-only">{t("domains.https")}</span> : null}
                  <a
                    href={hostUrl(domain)}
                    target="_blank"
                    rel="noreferrer noopener"
                    className="truncate type-mono hover:underline"
                    translate="no"
                  >
                    {domain.host}
                    <span className="sr-only"> {t("common.newTab")}</span>
                  </a>
                  <span className="flex items-center gap-1.5 type-meta text-muted-foreground">
                    <ArrowRightIcon className="size-3" aria-hidden="true" />
                    <Link
                      {...serviceLink(project, domain.service, { tab: "networking" })}
                      className="hover:text-foreground hover:underline"
                      translate="no"
                    >
                      {domain.service.name}
                    </Link>
                    <StatusDot health={statusOf(domain.service).health} className="size-1.5" />
                  </span>
                </div>
              </li>
            )
          })}
        </ul>
      )}
    </section>
  )
}

function Storage({
  project,
  data,
  refused,
}: {
  project: string
  data: Project
  refused: Schemas["UnreadableView"][]
}) {
  const t = useT()
  const { memoryParts } = useQuantityFormat()
  const claims = claimsOf(data)
  const sized = claims.map((claim) => ({ claim, bytes: claim.sizeBytes ?? undefined }))
  const total = sized.reduce((sum, c) => sum + (c.bytes ?? 0), 0)
  const anyKnown = sized.some((c) => c.bytes !== undefined)
  const [totalValue, totalUnit] = memoryParts(total)
  return (
    <section aria-labelledby="overview-storage" className="flex flex-col gap-1">
      <Heading
        id="overview-storage"
        title={t("tab.storage")}
        count={anyKnown ? t("storage.requestedTotal", `${totalValue} ${totalUnit}`) : undefined}
      />
      <RefusedKinds unreadable={refused} line />
      {claims.length === 0 ? (
        refused.length > 0 ? null : (
          <p className="border-t border-rule py-2.75 text-muted-foreground">{t("storage.none")}</p>
        )
      ) : (
        <>
          {total > 0 ? (
            // The volumes by size, in the design system's bar; each one's square below has its tone.
            <div className="my-1.5">
              <MeterStack parts={sized.map((c) => c.bytes ?? 0)} />
            </div>
          ) : null}
          <ul className="flex flex-col">
            {sized.map(({ claim, bytes }, i) => {
              const [value, unit] =
                bytes !== undefined ? memoryParts(bytes) : [undefined, undefined]
              return (
                <li key={claim.name} className={sideRow}>
                  <span
                    aria-hidden="true"
                    className={cn(
                      "size-2 shrink-0",
                      bytes ? meterPartTone(i) : "hairline-ring ring-idle ring-inset",
                    )}
                  />
                  <div className="flex min-w-0 flex-1 flex-col">
                    <span className="truncate type-mono" translate="no">
                      {claim.name}
                    </span>
                    {claim.services.length > 0 ? (
                      <span className="truncate type-meta text-muted-foreground">
                        {claim.services.map((service, j) => (
                          <span key={serviceId(service)}>
                            {j > 0 ? ", " : ""}
                            <Link
                              {...serviceLink(project, service, { tab: "storage" })}
                              className="hover:text-foreground hover:underline"
                              translate="no"
                            >
                              {service.name}
                            </Link>
                          </span>
                        ))}
                      </span>
                    ) : null}
                  </div>
                  {claim.services.length === 0 ? (
                    <Tooltip content={t("storage.notMounted")}>
                      <span className="flex shrink-0">
                        <StatusPill health="warn" size="sm" dot={false}>
                          {t("resource.unused")}
                        </StatusPill>
                      </span>
                    </Tooltip>
                  ) : value ? (
                    <span className="shrink-0">
                      {value} <span className="type-meta text-muted-foreground">{unit}</span>
                    </span>
                  ) : (
                    <span className="shrink-0 text-muted-foreground">{claim.size ?? "—"}</span>
                  )}
                </li>
              )
            })}
          </ul>
        </>
      )}
    </section>
  )
}

/** Backups across providers: one row per claim and database, then the namespace. Shown only
 * when the project has something to back up. */
function Backups({ project }: { project: string }) {
  const t = useT()
  const backups = useBackups(project)
  const data = backups.data
  if (!data) return null
  const targets = data.coverage.filter((c) => c.target.kind !== "namespace")
  if (targets.length === 0) return null
  const scheduled = targets.filter((c) => c.scheduledBy.length > 0).length
  return (
    <section aria-labelledby="overview-backups" className="flex flex-col gap-1">
      <Heading
        id="overview-backups"
        title={t("backups.title")}
        count={t("backups.scheduledOf", scheduled, targets.length)}
      />
      <ul className="flex flex-col">
        {shownCoverage(data.coverage).map((c) => (
          <CoverageRow key={`${c.target.kind}/${c.target.name}`} coverage={c} />
        ))}
      </ul>
      <ProviderNotes data={data} />
      <BackupDetails data={data} />
    </section>
  )
}
