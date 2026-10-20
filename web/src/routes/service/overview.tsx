import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import ArrowClockwiseIcon from "~icons/ph/arrow-clockwise"
import ArrowLeftIcon from "~icons/ph/arrow-left"
import ArrowRightIcon from "~icons/ph/arrow-right"
import ArrowUpRightIcon from "~icons/ph/arrow-up-right"
import CaretDownIcon from "~icons/ph/caret-down"
import CaretRightIcon from "~icons/ph/caret-right"
import CodeIcon from "~icons/ph/code"
import CubeIcon from "~icons/ph/cube"
import FileTextIcon from "~icons/ph/file-text"
import GitBranchIcon from "~icons/ph/git-branch"
import GlobeIcon from "~icons/ph/globe"
import TerminalWindowIcon from "~icons/ph/terminal-window"
import WarningCircleFillIcon from "~icons/ph/warning-circle-fill"

import { clusterQuery } from "@/api/queries"
import { Ago, CopyButton } from "@/components/bits"
import { Diagnoses } from "@/components/diagnoses"
import { EventList } from "@/components/events"
import { replicaCounts } from "@/components/pods"
import { ServiceIcon } from "@/components/service-icon"
import { plainReason, StatusDot, useStatusFacts, useStatusWord } from "@/components/status"
import { ActionTile } from "@zeabur/design-system/components/action-tile"
import { Badge } from "@zeabur/design-system/components/badge"
import { Button } from "@zeabur/design-system/components/button"
import { Container } from "@zeabur/design-system/components/container"
import { Popover, PopoverContent, PopoverTrigger } from "@zeabur/design-system/components/popover"
import {
  SummaryCard,
  SummaryCardActions,
  SummaryCardCount,
  SummaryCardFact,
  SummaryCardFacts,
  SummaryCardMain,
  SummaryCardSince,
  SummaryCardStatus,
  SummaryCardWord,
} from "@zeabur/design-system/components/summary-card"
import { Tooltip } from "@zeabur/design-system/components/tooltip"
import { useI18n, useT } from "@/i18n"
import { formatAgo, formatTime } from "@/lib/format"
import { serviceLink } from "@/lib/place"
import { cn } from "@/lib/utils"
import { neighboursOf, type Edge } from "@/model/connections"
import { edgeText } from "@/model/evidence"
import {
  domainsOf,
  hostUrl,
  imageTag,
  isHealthy,
  isTask,
  isWarning,
  podHealth,
  podReason,
  serviceId,
  statusOf,
  type Event,
  type Network,
  type Service,
} from "@/model/service"
import type { ServiceTab } from "@/routes/search"
import { UsageGlance } from "@/routes/service/usage"

type OnTab = (tab: ServiceTab, pod?: string) => void

export function OverviewTab({
  project,
  service,
  services,
  network,
  edges,
  events,
  onTab,
  onRestart,
}: {
  project: string
  service: Service
  services: Service[]
  network: Network
  edges: Edge[]
  events: Event[]
  onTab: OnTab
  onRestart?: () => void
}) {
  const t = useT()
  const diagnosed = (service.diagnoses?.length ?? 0) > 0
  const warnings = events.filter(isWarning)
  return (
    <div className="flex flex-col gap-5">
      {diagnosed ? (
        <Diagnoses diagnoses={service.diagnoses} service={service} onAction={onTab} />
      ) : null}
      <Summary
        project={project}
        service={service}
        diagnosed={diagnosed}
        onTab={onTab}
        onRestart={onRestart}
      />
      <div className="grid grid-cols-1 items-start gap-4 lg:grid-cols-2 lg:gap-5">
        <DomainsCard network={network} service={service} onTab={onTab} />
        <div className="grid grid-cols-3 gap-2 lg:gap-3">
          <ActionTile onClick={() => onTab("logs")}>
            <FileTextIcon aria-hidden="true" />
            {t("tab.logs")}
          </ActionTile>
          <ActionTile onClick={() => onTab("terminal")}>
            <TerminalWindowIcon aria-hidden="true" />
            {t("tab.terminal")}
          </ActionTile>
          <ActionTile onClick={() => onTab("yaml")}>
            <CodeIcon aria-hidden="true" />
            {t("tab.yaml")}
          </ActionTile>
        </div>
        {warnings.length > 0 ? (
          <Container
            title={t("overview.warnings")}
            titleAction={<More onClick={() => onTab("events")}>{t("common.all")}</More>}
            variant="list"
          >
            <div className="px-4">
              <EventList events={warnings} limit={3} compact />
            </div>
          </Container>
        ) : null}
        <ConnectionsCard project={project} service={service} services={services} edges={edges} />
      </div>
    </div>
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
  diagnosed,
  onTab,
  onRestart,
}: {
  project: string
  service: Service
  diagnosed: boolean
  onTab: OnTab
  onRestart?: () => void
}) {
  const t = useT()
  const { locale } = useI18n()
  const word = useStatusWord()
  const cluster = useQuery(clusterQuery).data?.name || undefined
  const status = statusOf(service)
  const facts = useStatusFacts()(status.reason, status.pod)
  const started = service.pods
    .map((pod) => pod.created)
    .filter((created): created is string => !!created)
    .toSorted()
    .at(-1)
  const since = started ?? service.created ?? undefined
  return (
    <SummaryCard aria-label={t("common.status")}>
      <SummaryCardMain>
        <SummaryCardStatus tone={status.health}>
          <SummaryCardWord>{word(status)}</SummaryCardWord>
          {status.desired !== undefined ? <Replicas service={service} onTab={onTab} /> : null}
        </SummaryCardStatus>
        {since ? (
          <SummaryCardSince>
            <Tooltip content={formatTime(since, locale)}>
              <time dateTime={since}>{t("overview.started", formatAgo(since, locale))}</time>
            </Tooltip>
          </SummaryCardSince>
        ) : null}
        {facts && !isHealthy(status) ? (
          <p className="mt-1 text-sm text-secondary">{facts}</p>
        ) : null}
        {!isHealthy(status) && !diagnosed ? <Exceptions service={service} /> : null}
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
          {cluster ? (
            <SummaryCardFact label={t("overview.cluster")}>
              <span className="truncate font-mono" translate="no">
                {cluster}
              </span>
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
        {onRestart ? (
          <SummaryCardActions>
            <Button variant="outline" size="sm" onClick={onRestart}>
              <ArrowClockwiseIcon aria-hidden="true" data-icon="inline-start" />
              {t("ops.restart")}
            </Button>
          </SummaryCardActions>
        ) : null}
      </SummaryCardMain>
      {isTask(service.kind) ? null : (
        <UsageGlance
          project={project}
          service={service}
          action={<More onClick={() => onTab("metrics")}>{t("tab.metrics")}</More>}
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

function Replicas({ service, onTab }: { service: Service; onTab: OnTab }) {
  const t = useT()
  const { ready, desired } = replicaCounts(service)
  const task = isTask(service.kind)
  const scalable = service.kind === "Deployment" || service.kind === "StatefulSet"
  return (
    <Popover>
      <PopoverTrigger
        render={<SummaryCardCount aria-label={t("overview.replicas", ready, desired)} />}
      >
        {ready}/{desired}
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
                    <button
                      type="button"
                      onClick={() => onTab("logs", pod.name)}
                      className="shrink-0 rounded px-1 text-muted-foreground outline-none hover:bg-muted hover:text-foreground"
                      aria-label={t("pod.logsOf", pod.name)}
                    >
                      {t("tab.logs")}
                    </button>
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
            <More onClick={() => onTab("settings")}>{t("overview.replicaSettings")}</More>
          </div>
        ) : null}
      </PopoverContent>
    </Popover>
  )
}

function DomainsCard({
  network,
  service,
  onTab,
}: {
  network: Network
  service: Service
  onTab: OnTab
}) {
  const t = useT()
  const domains = domainsOf(network, service)
  if (domains.length === 0) return null
  return (
    <Container
      title={t("common.domains")}
      titleAction={<More onClick={() => onTab("networking")}>{t("overview.manage")}</More>}
      variant="list"
    >
      {domains.map((domain) => (
        <a
          key={domain.host}
          href={hostUrl(domain)}
          target="_blank"
          rel="noreferrer noopener"
          className="flex w-full items-center gap-3 px-4 py-3 transition-colors hover:bg-v2-surface-hover"
        >
          <span className="min-w-0 flex-1 truncate text-sm font-medium" translate="no">
            {domain.host}
          </span>
          {domain.https ? (
            <Tooltip content={t("domains.https")}>
              <Badge variant="ok" translate="no">
                HTTPS
              </Badge>
            </Tooltip>
          ) : (
            <Badge variant="secondary" translate="no">
              HTTP
            </Badge>
          )}
          <ArrowUpRightIcon className="size-4 shrink-0 text-tertiary" aria-hidden="true" />
        </a>
      ))}
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
