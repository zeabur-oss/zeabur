import { Link } from "@tanstack/react-router"
import ArrowLeftIcon from "~icons/ph/arrow-left"
import ArrowRightIcon from "~icons/ph/arrow-right"
import ArrowSquareOutIcon from "~icons/ph/arrow-square-out"
import CheckCircleFillIcon from "~icons/ph/check-circle-fill"
import GlobeIcon from "~icons/ph/globe"
import LockSimpleBoldIcon from "~icons/ph/lock-simple-bold"
import WarningCircleFillIcon from "~icons/ph/warning-circle-fill"

import { Ago, Panel } from "@/components/bits"
import { Diagnoses } from "@/components/diagnoses"
import { EventList } from "@/components/events"
import { ServiceIcon } from "@/components/service-icon"
import { FactsPill, plainReason, StatusDot, useStatusFacts } from "@/components/status"
import { Tooltip } from "@zeabur/design-system/components/tooltip"
import { useT } from "@/i18n"
import { serviceLink } from "@/lib/place"
import { cn } from "@/lib/utils"
import { neighboursOf, type Edge } from "@/model/connections"
import { edgeText } from "@/model/evidence"
import {
  domainsOf,
  hostUrl,
  podFacts,
  podHealth,
  podReason,
  serviceId,
  type Event,
  type Network,
  type Service,
} from "@/model/service"
import type { ServiceTab } from "@/routes/search"

export function OverviewTab({
  project,
  service,
  services,
  network,
  edges,
  events,
  onTab,
}: {
  project: string
  service: Service
  services: Service[]
  network: Network
  edges: Edge[]
  events: Event[]
  onTab: (tab: ServiceTab, pod?: string) => void
}) {
  const t = useT()
  return (
    <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
      {service.diagnoses && service.diagnoses.length > 0 ? (
        <div className="lg:col-span-2">
          <Diagnoses diagnoses={service.diagnoses} service={service} onAction={onTab} />
        </div>
      ) : null}
      <StatusCard service={service} />
      <PodsCard service={service} onTab={onTab} />
      <ConnectionsCard project={project} service={service} services={services} edges={edges} />
      <DomainsCard network={network} service={service} onTab={onTab} />
      <Panel
        className="lg:col-span-2"
        title={t("project.events")}
        action={<TabLink onClick={() => onTab("events")}>{t("common.all")}</TabLink>}
      >
        <EventList events={events} limit={4} compact />
      </Panel>
    </div>
  )
}

function TabLink({ onClick, children }: { onClick: () => void; children: string }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="flex items-center gap-1 rounded text-xs text-muted-foreground outline-none hover:text-foreground"
    >
      {children}
    </button>
  )
}

function StatusCard({ service }: { service: Service }) {
  const t = useT()
  const ready = service.replicas?.ready ?? 0
  const desired = service.replicas?.desired ?? 0
  const restarting = service.pods.filter((p) => p.restarts > 0).length
  const conditions: { ok: boolean; label: string; detail: string }[] =
    service.kind === "CronJob"
      ? [
          {
            ok: service.phase !== "Suspended",
            label: t("cond.scheduled"),
            detail: t(`phase.${service.phase}`),
          },
        ]
      : [
          { ok: ready > 0, label: t("cond.available"), detail: "" },
          { ok: ready >= desired, label: t("cond.allReady"), detail: `${ready}/${desired}` },
          {
            ok: !service.pods.some((pod) => pod.health === "fail"),
            label: t("cond.stable"),
            detail: restarting > 0 ? t("cond.restarting", restarting) : t("cond.noRestarts"),
          },
        ]
  return (
    <Panel title={t("common.status")}>
      <ul className="flex flex-col gap-2.5">
        {conditions.map((c) => (
          <li key={c.label} className="flex items-center gap-2.5 text-sm">
            {c.ok ? (
              <CheckCircleFillIcon className="size-4 text-ok" aria-hidden="true" />
            ) : (
              <WarningCircleFillIcon className="size-4 text-fail" aria-hidden="true" />
            )}
            <span className="font-medium">{c.label}</span>
            <span className="sr-only">{c.ok ? t("common.yes") : t("common.no")}</span>
            <span className="ml-auto text-xs text-muted-foreground tabular-nums">{c.detail}</span>
          </li>
        ))}
      </ul>
    </Panel>
  )
}

function PodsCard({
  service,
  onTab,
}: {
  service: Service
  onTab: (tab: ServiceTab, pod?: string) => void
}) {
  const t = useT()
  const statusFacts = useStatusFacts()
  const task = service.kind === "Job" || service.kind === "CronJob"
  const ready = service.pods.filter((p) => p.ready).length
  return (
    <Panel
      title={t("pod.pods")}
      action={
        service.pods.length > 0 ? (
          <span className="text-xs text-muted-foreground tabular-nums">
            {t("pod.readyCount", ready, service.pods.length)}
          </span>
        ) : undefined
      }
      bodyClassName="p-0"
    >
      {service.pods.length === 0 ? (
        <p className="px-5 py-3 text-sm text-muted-foreground">{t("pod.none")}</p>
      ) : (
        <ul className="flex flex-col divide-y">
          {service.pods.map((pod) => {
            const health = podHealth(pod)
            const reason = podReason(pod)
            return (
              <li
                key={pod.name}
                className="flex flex-wrap items-center gap-x-2 gap-y-1 px-5 py-2 text-xs"
              >
                <StatusDot health={health} />
                <span className="min-w-0 truncate font-mono" translate="no">
                  {pod.name}
                </span>
                {reason || !pod.ready || health === "fail" ? (
                  <FactsPill
                    health={health === "fail" ? "fail" : "warn"}
                    size="sm"
                    dot={false}
                    facts={statusFacts(reason, podFacts(pod))}
                  >
                    {plainReason(t, reason, task) ??
                      // Ready now, failing for a recent crash (the health rule's crash loop).
                      (pod.ready ? t("reason.crashing") : t("pod.notReady"))}
                  </FactsPill>
                ) : null}
                <span className="ml-auto flex shrink-0 items-center gap-3 text-muted-foreground">
                  {pod.node ? (
                    <span
                      className="hidden font-mono sm:inline"
                      translate="no"
                      title={t("pod.node")}
                    >
                      {pod.node}
                    </span>
                  ) : null}
                  {pod.created ? <Ago iso={pod.created} /> : null}
                  <span className={cn("tabular-nums", pod.restarts > 0 && "text-foreground")}>
                    {t("pod.restarts", pod.restarts)}
                  </span>
                  <button
                    type="button"
                    onClick={() => onTab("logs", pod.name)}
                    className="rounded px-1.5 py-0.5 hover:bg-muted hover:text-foreground"
                    aria-label={t("pod.logsOf", pod.name)}
                  >
                    {t("tab.logs")}
                  </button>
                </span>
              </li>
            )
          })}
        </ul>
      )}
    </Panel>
  )
}

function DomainsCard({
  network,
  service,
  onTab,
}: {
  network: Network
  service: Service
  onTab: (tab: ServiceTab) => void
}) {
  const t = useT()
  const domains = domainsOf(network, service)
  return (
    <Panel
      title={t("common.domains")}
      action={<TabLink onClick={() => onTab("networking")}>{t("tab.networking")}</TabLink>}
    >
      {domains.length === 0 ? (
        <p className="text-sm text-muted-foreground">
          {service.endpointNames.length > 0 ? t("domains.privateOnly") : t("domains.none")}
        </p>
      ) : (
        <ul className="flex flex-col gap-2">
          {domains.map((domain) => (
            <li key={domain.host} className="flex items-center gap-2 text-sm">
              {domain.https ? (
                <Tooltip content={t("domains.https")}>
                  <LockSimpleBoldIcon
                    className="size-3.5 text-ok"
                    aria-label={t("domains.https")}
                    role="img"
                  />
                </Tooltip>
              ) : (
                <GlobeIcon className="size-3.5 text-muted-foreground" aria-hidden="true" />
              )}
              <a
                href={hostUrl(domain)}
                target="_blank"
                rel="noreferrer noopener"
                className="truncate font-mono text-xs hover:underline"
                translate="no"
              >
                {domain.host}
              </a>
              <ArrowSquareOutIcon className="size-3 text-muted-foreground" aria-hidden="true" />
            </li>
          ))}
        </ul>
      )}
    </Panel>
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
  const row = (edge: Edge, direction: "out" | "in") => {
    const otherId = direction === "out" ? edge.to : edge.from
    const other = services.find((s) => serviceId(s) === otherId)
    const DirectionIcon = direction === "out" ? ArrowRightIcon : ArrowLeftIcon
    return (
      <li key={edge.id} className="flex items-center gap-2.5 py-1.5 text-sm">
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
      </li>
    )
  }
  return (
    <Panel title={t("connections.title")}>
      {upstream.length + downstream.length === 0 ? (
        <p className="text-sm text-muted-foreground">{t("connections.none")}</p>
      ) : (
        <ul className="flex flex-col divide-y">
          {downstream.map((e) => row(e, "out"))}
          {upstream.map((e) => row(e, "in"))}
        </ul>
      )}
    </Panel>
  )
}
