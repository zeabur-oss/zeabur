import { useQuery, type UseQueryResult } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import ArrowLeftIcon from "~icons/ph/arrow-left"
import ArrowRightIcon from "~icons/ph/arrow-right"
import CaretDownIcon from "~icons/ph/caret-down"
import CaretRightIcon from "~icons/ph/caret-right"
import CubeIcon from "~icons/ph/cube"
import GitBranchIcon from "~icons/ph/git-branch"
import GlobeIcon from "~icons/ph/globe"
import WarningCircleFillIcon from "~icons/ph/warning-circle-fill"

import { useLogsReadable } from "@/api/logs-access"
import { projectAlertsQuery, rolloutQuery, traceQuery } from "@/api/queries"
import { AlertList, RecentNote } from "@/components/alerts"
import { Ago, CopyButton } from "@/components/bits"
import { Diagnoses } from "@/components/diagnoses"
import { EventList } from "@/components/events"
import { replicaCounts } from "@/components/pods"
import { RouteLine } from "@/components/route-trace"
import { ServiceIcon } from "@/components/service-icon"
import { useBrowserChecks } from "@/components/browser-check"
import { plainReason, StatusDot, useStatusFacts } from "@/components/status"
import { ReachLine, reachSetup, useHeadline } from "@/components/verdict"
import { Button } from "@zeabur-oss/design-system/components/button"
import { Container } from "@zeabur-oss/design-system/components/container"
import { Meter } from "@zeabur-oss/design-system/components/meter"
import { Popover, PopoverContent, PopoverTrigger } from "@zeabur-oss/design-system/components/popover"
import {
  SummaryCard,
  SummaryCardCount,
  SummaryCardFact,
  SummaryCardFacts,
  SummaryCardMain,
  SummaryCardSince,
  SummaryCardStatus,
  SummaryCardWord,
} from "@zeabur-oss/design-system/components/summary-card"
import { SwapText } from "@zeabur-oss/design-system/components/swap-text"
import { TermDetail } from "@zeabur-oss/design-system/components/term"
import { Tooltip } from "@zeabur-oss/design-system/components/tooltip"
import { useI18n, useSentence, useT } from "@/i18n"
import { LoadingRows } from "@/components/page"
import { Problem } from "@/components/problem"
import { GateTip, useGate } from "@/components/gate"
import { usePlanner } from "@/components/plan/planner"
import { formatAgo, formatTime } from "@/lib/format"
import { serviceLink } from "@/lib/place"
import { cn } from "@/lib/utils"
import type { Schemas } from "@/api/client"
import { alertsFor, folded } from "@/model/alerts"
import { failureText, refusedCredentials, runsOf } from "@/model/builds"
import { neighboursOf, type Edge } from "@/model/connections"
import { edgeText } from "@/model/evidence"
import { overviewOf, type Overview } from "@/model/overview"
import { verdictOf, type Verdict } from "@/model/verdict"
import {
  imageTag,
  isHealthy,
  isTask,
  podHealth,
  podReason,
  serviceId,
  statusOf,
  type Event,
  type Service,
} from "@/model/service"
import type { ServicePlace } from "@/routes/search"
import { UploadAction, useServiceBuilds } from "@/routes/service/builds"
import { TaskRunsCard } from "@/routes/service/tasks"
import { UsageGlance } from "@/routes/service/usage"

type OnTab = (place: ServicePlace, pod?: string) => void
type RolloutProgress = Schemas.RolloutProgress
type ServiceTrace = Schemas.ServiceTrace

/** Health first, then each cause once with what it takes down, then how the service is reached. */
export function OverviewTab({
  project,
  service,
  services,
  edges,
  events,
  onTab,
}: {
  project: string
  service: Service
  services: Service[]
  edges: Edge[]
  events: Event[]
  onTab: OnTab
}) {
  const t = useT()
  const task = isTask(service.kind)
  const rollout = useQuery({
    ...rolloutQuery(project, service.kind, service.name),
    enabled: !task,
  })
  const trace = useQuery(traceQuery(project, service.kind, service.name))
  const said = overviewOf(
    { diagnoses: service.diagnoses, stopped: service.replicas?.desired === 0 },
    task ? undefined : rollout.data,
    trace.data?.routes ?? [],
    events,
  )
  const diagnosed = said.causes.length > 0
  const browser = useBrowserChecks()
  const routes = trace.data?.routes ?? []
  const verdict = verdictOf({
    status: statusOf(service),
    said,
    routes,
    pods: service.pods,
    checks: (address) => {
      const route = routes.find((r) => r.kind === "domain" && r.address === address)
      return browser(address, route?.hops.some((h) => h.step === "certificate") ?? false)
    },
  })
  return (
    <div className="flex flex-col gap-5">
      <Summary
        project={project}
        service={service}
        verdict={verdict}
        explained={diagnosed || said.rollout.as === "card"}
        onTab={onTab}
      />
      {diagnosed ? (
        <Diagnoses
          diagnoses={said.causes}
          service={service}
          project={project}
          also={
            <AlsoMeans project={project} service={service} said={said} rollout={rollout.data} />
          }
        />
      ) : null}
      {service.kind === "CronJob" ? <TaskRunsCard project={project} service={service} /> : null}
      {said.rollout.as === "card" && rollout.data && !buildSays(service) ? (
        <Rollout project={project} service={service} data={rollout.data} />
      ) : null}
      <div className="grid grid-cols-1 items-start gap-4 lg:grid-cols-2 lg:gap-5">
        <Reachability
          project={project}
          service={service}
          trace={trace}
          waiting={said.waiting}
          onTab={onTab}
        />
        <div className="flex min-w-0 flex-col gap-4 lg:gap-5">
          {said.warnings.length > 0 ? (
            <Container
              title={t("overview.warnings")}
              titleAction={<More onClick={() => onTab("events")}>{t("common.all")}</More>}
              variant="list"
            >
              <div className="px-4">
                <EventList events={said.warnings} limit={3} compact plain />
              </div>
            </Container>
          ) : null}
          <AlertsCard project={project} service={service} />
          <ConnectionsCard project={project} service={service} services={services} edges={edges} />
        </div>
      </div>
    </div>
  )
}

const ALERTS_SHOWN = 3

/** What Falco saw in this service's containers; the project page lists them all. */
function AlertsCard({ project, service }: { project: string; service: Service }) {
  const t = useT()
  const data = useQuery(projectAlertsQuery(project)).data
  const alerts = data?.state === "installed" ? alertsFor(data.items, service) : []
  if (alerts.length === 0) return null
  return (
    <Container
      title={t("alerts.title")}
      titleAction={
        folded(alerts).length > ALERTS_SHOWN ? (
          <Link
            to="/projects/$project"
            params={{ project }}
            className="flex items-center gap-0.5 rounded text-xs font-medium text-secondary outline-none hover:text-main"
          >
            {t("common.all")}
            <CaretRightIcon className="size-3.5" aria-hidden="true" />
          </Link>
        ) : undefined
      }
      variant="list"
    >
      <div className="flex flex-col gap-2 px-4 pb-3">
        <AlertList alerts={alerts} limit={ALERTS_SHOWN} />
        <RecentNote />
      </div>
    </Container>
  )
}

/** What the first cause also takes down: the addresses waiting on it, and a release stuck on it. */
function AlsoMeans({
  project,
  service,
  said,
  rollout,
}: {
  project: string
  service: Service
  said: Overview
  rollout?: RolloutProgress
}) {
  const t = useT()
  const planner = usePlanner(project)
  const gate = useGate(project, "changeServices")
  const down = said.alsoDown
  const line = said.rollout.as === "line" && rollout ? said.rollout : undefined
  if (!down && !line) return null
  const rollback = rollout?.rollback
  return (
    <>
      {down ? (
        <p>
          <span className="font-medium">
            {t("overview.alsoMeans")}
            {t("common.colon")}
          </span>
          {t(
            "overview.alsoDown",
            t("common.list", [
              ...down.domains,
              ...(down.private ? [t("overview.privateAddress")] : []),
            ]),
            service.name,
          )}
        </p>
      ) : null}
      {line && rollout ? (
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5">
          <p className="min-w-0 flex-1 basis-60">
            <span className="font-medium">
              {t(line.stuck ? "rollout.stuckLine" : "rollout.waitsLine")}
              {t("common.colon")}
            </span>
            <span className="tabular-nums">
              {t("rollout.ready", rollout.updatedReady, rollout.desired)}
            </span>
          </p>
          {rollback ? (
            <GateTip gate={gate}>
              <Button
                variant="outline"
                size="sm"
                onClick={() =>
                  void planner.plan({
                    title: t("rollout.rollBack", service.name),
                    changes: rollback.changes,
                    service: { kind: service.kind, name: service.name },
                  })
                }
              >
                {t("rollout.rollBackButton")}
              </Button>
            </GateTip>
          ) : null}
          {planner.dialog}
        </div>
      ) : null}
    </>
  )
}

const rolloutTone = {
  progressing: "border-progress/30 bg-progress-soft/30",
  waitsForDeletion: "border-progress/30 bg-progress-soft/30",
  paused: "border-warn/30 bg-warn-soft/30",
  stuck: "border-fail/30 bg-fail-soft/30",
  done: "",
} as const

/**
 * How far the newest release has come, from its own replicas. Without the releases (ReplicaSets
 * the person may not read) its replicas can't be told from older ones, so the service's are said.
 */
function Rollout({
  project,
  service,
  data,
}: {
  project: string
  service: Service
  data: RolloutProgress
}) {
  const t = useT()
  const write = useSentence()
  const planner = usePlanner(project)
  const gate = useGate(project, "changeServices")
  const rollback = data.rollback
  const known = data.revision != null
  const counts = replicaCounts(service)
  const ready = known ? data.updatedReady : counts.ready
  const progress = known
    ? t("rollout.ready", data.updatedReady, data.desired)
    : t("overview.someReady", counts.ready, data.desired)
  return (
    <section
      aria-label={t("rollout.title")}
      aria-live="polite"
      className={cn("flex flex-col gap-2 rounded-lg border px-4 py-3", rolloutTone[data.status])}
    >
      <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1 text-sm">
        <span className="font-medium">
          <SwapText>
            {known || data.status === "stuck"
              ? write(data.message)
              : t("rollout.rollingOut", service.name)}
          </SwapText>
        </span>
        {data.revision ? (
          <span className="text-secondary">{t("changes.release", data.revision)}</span>
        ) : null}
        <span className="text-secondary tabular-nums">
          <SwapText variant="number">{progress}</SwapText>
        </span>
        {data.old > 0 ? (
          <span className="text-secondary">{t("rollout.older", data.old)}</span>
        ) : null}
        {rollback ? (
          <span className="ml-auto">
            <GateTip gate={gate}>
              <Button
                variant="outline"
                size="sm"
                onClick={() =>
                  void planner.plan({
                    title: t("rollout.rollBack", service.name),
                    changes: rollback.changes,
                    service: { kind: service.kind, name: service.name },
                  })
                }
              >
                {t("rollout.rollBackButton")}
              </Button>
            </GateTip>
          </span>
        ) : null}
      </div>
      <Meter value={Math.min(ready, data.desired) / data.desired} label={progress} />
      {planner.dialog}
    </section>
  )
}

const buildSays = (service: Service) =>
  ["building", "waitingUpload"].includes(statusOf(service).word) ||
  statusOf(service).reason === "BuildFailed"

/** The build of a service waiting for its first image, under way or failed, or of one whose
 * latest build failed: why. */
function BuildLine({
  project,
  service,
  onTab,
}: {
  project: string
  service: Service
  onTab: OnTab
}) {
  const t = useT()
  const { builds, built } = useServiceBuilds(project, service)
  const build = built[0]?.build
  const run = build ? runsOf(builds.data?.runs ?? [], build)[0] : undefined
  const failed = statusOf(service).reason === "BuildFailed"
  const waiting = !failed && statusOf(service).word === "waitingUpload"
  const refused = build ? refusedCredentials(build, run) : undefined
  const why = failed
    ? ((refused ? t(`builds.refused.${refused}`) : undefined) ?? failureText(build, run))
    : waiting
      ? t("builds.waitingUpload", service.name)
      : t("overview.building")
  return (
    <p className="mt-1 flex max-w-prose flex-wrap items-baseline gap-x-2 gap-y-1 text-sm text-secondary">
      {why ? <span className="min-w-0 break-words">{why}</span> : null}
      {waiting && build?.archive ? (
        <UploadAction project={project} build={build.name} primary />
      ) : null}
      <More onClick={() => onTab("builds")}>{t("overview.seeBuild")}</More>
    </p>
  )
}

function More({ onClick, children }: { onClick: () => void; children: string }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="flex items-center gap-0.5 rounded text-xs font-medium text-secondary outline-none hover:text-main"
    >
      {children}
      <CaretRightIcon className="size-3.5" aria-hidden="true" />
    </button>
  )
}

function Summary({
  project,
  service,
  verdict,
  explained,
  onTab,
}: {
  project: string
  service: Service
  verdict: Verdict
  /** A diagnosis or a rollout below already says what is missing. */
  explained: boolean
  onTab: OnTab
}) {
  const t = useT()
  const { locale } = useI18n()
  const headline = useHeadline()
  const status = statusOf(service)
  const facts = useStatusFacts()(undefined, status.pod)
  const term = status.pod?.running ? undefined : status.reason
  const started = service.pods
    .map((pod) => pod.created)
    .filter((created): created is string => !!created)
    .toSorted()
    .at(-1)
  const since = started ?? service.created ?? undefined
  const setup = verdict.reach ? reachSetup(verdict.reach) : undefined
  const buildSaid = buildSays(service)
  return (
    <SummaryCard aria-label={t("common.status")}>
      <SummaryCardMain>
        <SummaryCardStatus tone={verdict.tone}>
          <SummaryCardWord>
            <SwapText>{headline(verdict.headline, status)}</SwapText>
          </SummaryCardWord>
          {status.desired !== undefined ? (
            <Replicas project={project} service={service} onTab={onTab} />
          ) : null}
        </SummaryCardStatus>
        {verdict.reach ? (
          <ReachLine
            reach={verdict.reach}
            more={setup ? <More onClick={() => onTab("networking")}>{t(setup)}</More> : undefined}
          />
        ) : null}
        {since ? (
          <SummaryCardSince>
            <Tooltip content={formatTime(since, locale)}>
              <time dateTime={since}>{t("overview.started", formatAgo(since, locale))}</time>
            </Tooltip>
          </SummaryCardSince>
        ) : null}
        {buildSaid ? <BuildLine project={project} service={service} onTab={onTab} /> : null}
        {(facts || term) && !isHealthy(status) && !buildSaid ? (
          <p className="mt-1 flex flex-wrap items-baseline gap-x-2 text-sm text-secondary">
            {facts}
            {term ? <TermDetail translate="no">{term}</TermDetail> : null}
          </p>
        ) : null}
        {!isHealthy(status) && !explained && !buildSaid ? <Exceptions service={service} /> : null}
        {status.unchecked ? (
          <p className="mt-2 flex max-w-prose flex-wrap items-baseline gap-x-2 gap-y-1 text-sm text-secondary">
            {t("status.uncheckedWhy")}
            <More onClick={() => onTab("health-checks")}>{t("status.addTrafficCheck")}</More>
          </p>
        ) : null}
        <SummaryCardFacts>
          {service.image ? (
            <SummaryCardFact label={t("overview.source")}>
              <CubeIcon aria-hidden="true" />
              <span className="flex min-w-0 font-mono" title={service.image} translate="no">
                <span className="truncate">{service.image.replace(/[:@][^:/@]+$/, "")}</span>
                <span className="shrink-0">:{imageTag(service.image)}</span>
              </span>
              <CopyButton value={service.image} />
            </SummaryCardFact>
          ) : null}
          {service.provenance.tool ? (
            <SummaryCardFact label={t("overview.managedBy")}>
              <GitBranchIcon aria-hidden="true" />
              <Tooltip content={t("common.managedByDetail", service.provenance.tool)}>
                <span className="cursor-help truncate">{service.provenance.tool}</span>
              </Tooltip>
            </SummaryCardFact>
          ) : null}
        </SummaryCardFacts>
      </SummaryCardMain>
      {isTask(service.kind) ? null : (
        <UsageGlance
          project={project}
          service={service}
          action={<More onClick={() => onTab("usage")}>{t("tab.usage")}</More>}
        />
      )}
    </SummaryCard>
  )
}

const exceptionTone = {
  fail: "text-fail-strong",
  warn: "text-warn-strong",
  progress: "text-secondary",
  ok: "text-secondary",
  idle: "text-secondary",
} as const

function Exceptions({ service }: { service: Service }) {
  const t = useT()
  const { ready, desired } = replicaCounts(service)
  const crashing = service.pods.filter((pod) => pod.health === "fail").length
  const lines = [
    desired > 0 && ready === 0 ? t("overview.noneReady") : undefined,
    ready > 0 && ready < desired ? t("overview.someReady", ready, desired) : undefined,
    crashing > 0 ? t("overview.crashing", crashing) : undefined,
  ].filter((line): line is string => !!line)
  if (lines.length === 0) return null
  return (
    <ul className="mt-3 flex flex-col gap-1">
      {lines.map((line) => (
        <li
          key={line}
          className={cn("flex items-center gap-1.5 text-sm", exceptionTone[service.status.health])}
        >
          <WarningCircleFillIcon className="size-3.5 shrink-0" aria-hidden="true" />
          {line}
        </li>
      ))}
    </ul>
  )
}

function Replicas({
  project,
  service,
  onTab,
}: {
  project: string
  service: Service
  onTab: OnTab
}) {
  const t = useT()
  const logs = useLogsReadable(project)
  const { ready, desired } = replicaCounts(service)
  const task = isTask(service.kind)
  const scalable = service.kind === "Deployment" || service.kind === "StatefulSet"
  return (
    <Popover>
      <PopoverTrigger
        render={<SummaryCardCount aria-label={t("overview.replicas", ready, desired)} />}
      >
        <SwapText variant="number">{`${ready}/${desired}`}</SwapText>
        <CaretDownIcon aria-hidden="true" />
      </PopoverTrigger>
      <PopoverContent>
        {service.pods.length === 0 ? (
          <p className="px-2 py-1.5 text-xs text-muted-foreground">{t("pod.none")}</p>
        ) : (
          <ul className="flex max-h-80 flex-col overflow-y-auto">
            {service.pods.map((pod) => {
              const health = podHealth(pod)
              const reason =
                plainReason(t, podReason(pod), task) ??
                (pod.ready ? t("pod.ready") : t("pod.notReady"))
              return (
                <li key={pod.name} className="flex flex-col gap-0.5 rounded px-2 py-1.5">
                  <span className="flex min-w-0 items-center gap-2 text-xs">
                    <StatusDot health={health} />
                    <span className="min-w-0 flex-1 truncate font-mono" translate="no">
                      {pod.name}
                    </span>
                    {logs ? (
                      <button
                        type="button"
                        onClick={() => onTab("logs", pod.name)}
                        className="shrink-0 rounded px-1 text-muted-foreground outline-none hover:bg-muted hover:text-foreground"
                        aria-label={t("pod.logsOf", pod.name)}
                      >
                        {t("tab.logs")}
                      </button>
                    ) : null}
                  </span>
                  <span className="flex flex-wrap gap-x-2 pl-3.5 type-meta text-muted-foreground">
                    <span className={health === "ok" ? undefined : "text-foreground"}>
                      {reason}
                    </span>
                    {pod.node ? (
                      <span className="font-mono" translate="no">
                        {pod.node}
                      </span>
                    ) : null}
                    {pod.created ? <Ago iso={pod.created} /> : null}
                    {pod.restarts > 0 ? <span>{t("pod.restarts", pod.restarts)}</span> : null}
                  </span>
                </li>
              )
            })}
          </ul>
        )}
        {scalable ? (
          <div className="mt-1 border-t border-v2-border px-2 pt-2 pb-1">
            <More onClick={() => onTab("compute")}>{t("overview.replicaSettings")}</More>
          </div>
        ) : null}
      </PopoverContent>
    </Popover>
  )
}

function Reachability({
  project,
  service,
  trace,
  waiting,
  onTab,
}: {
  project: string
  service: Service
  trace: UseQueryResult<ServiceTrace>
  waiting: Overview["waiting"]
  onTab: OnTab
}) {
  const t = useT()
  const planner = usePlanner(project)
  const routes = trace.data?.routes ?? []
  if (trace.isSuccess && routes.length === 0) return null
  return (
    <Container
      title={t("tab.networking")}
      titleAction={<More onClick={() => onTab("networking")}>{t("overview.manage")}</More>}
      variant="list"
    >
      {trace.isPending ? (
        <div className="px-4 py-3">
          <LoadingRows label={t("common.loading")} rows={2} />
        </div>
      ) : trace.isError ? (
        <div className="px-4 py-3">
          <Problem error={trace.error} />
        </div>
      ) : null}
      {routes.map((route) => {
        const fix = route.fix
        const waits = waiting.get(route.address)
        return (
          <RouteLine
            key={`${route.kind}/${route.address}`}
            route={route}
            waiting={
              waits === "cause"
                ? t("trace.waiting", service.name)
                : waits === "stopped"
                  ? t("trace.stopped", service.name)
                  : undefined
            }
            fix={
              waits
                ? undefined
                : trace.data?.diagnoses.find((d) => d.cause.params?.address === route.address)?.fix
            }
            onFix={
              fix && !waits
                ? () =>
                    void planner.plan({
                      title: t("trace.fixTitle", route.address),
                      changes: fix.changes,
                      service: { kind: service.kind, name: service.name },
                    })
                : undefined
            }
          />
        )
      })}
      {planner.dialog}
    </Container>
  )
}

function ConnectionsCard({
  project,
  service,
  services,
  edges,
}: {
  project: string
  service: Service
  services: Service[]
  edges: Edge[]
}) {
  const t = useT()
  const { upstream, downstream } = neighboursOf(edges, service)
  if (upstream.length + downstream.length === 0) return null
  const row = (edge: Edge, direction: "out" | "in") => {
    const otherId = direction === "out" ? edge.to : edge.from
    const other = services.find((s) => serviceId(s) === otherId)
    const DirectionIcon = direction === "out" ? ArrowRightIcon : ArrowLeftIcon
    return (
      <div key={edge.id} className="flex items-center gap-2.5 px-4 py-2.5 text-sm">
        <DirectionIcon className="size-3.5 shrink-0 text-muted-foreground" aria-hidden="true" />
        {other ? (
          <Link
            {...serviceLink(project, other)}
            className="flex shrink-0 items-center gap-2 font-medium hover:underline"
            translate="no"
          >
            <ServiceIcon service={other} size="sm" />
            {other.name}
          </Link>
        ) : (
          <span className="flex shrink-0 items-center gap-2 font-mono text-xs" translate="no">
            <GlobeIcon className="size-4 text-muted-foreground" aria-hidden="true" />
            {otherId.replace(/^domain:/, "")}
          </span>
        )}
        <span
          className="min-w-0 flex-1 truncate text-xs text-muted-foreground"
          title={edgeText(edge, t)}
        >
          {edgeText(edge, t)}
        </span>
        {edge.inferred ? (
          <span className="shrink-0 type-meta text-muted-foreground">{t("edge.inferred")}</span>
        ) : null}
      </div>
    )
  }
  return (
    <Container title={t("connections.title")} variant="list">
      {downstream.map((e) => row(e, "out"))}
      {upstream.map((e) => row(e, "in"))}
    </Container>
  )
}
