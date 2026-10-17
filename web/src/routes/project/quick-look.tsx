// The Architecture quick look: a service's status, pods, what it depends on and what depends on
// it, and its latest events, over the canvas. Logs, variables and the rest are on the service page.
import { Link } from "@tanstack/react-router"
import { useEffect } from "react"
import ArrowLeftIcon from "~icons/ph/arrow-left"
import ArrowRightIcon from "~icons/ph/arrow-right"
import ArrowSquareOutIcon from "~icons/ph/arrow-square-out"
import GlobeIcon from "~icons/ph/globe"
import XIcon from "~icons/ph/x"

import { EventList } from "@/components/events"
import { ServiceIcon } from "@/components/service-icon"
import { ServiceStatus, StatusDot } from "@/components/status"
import { Button, buttonVariants } from "@/components/ui/button"
import { useT } from "@/i18n"
import { podLogsLink, serviceLink } from "@/lib/place"
import { cn } from "@/lib/utils"
import { neighboursOf, type Edge } from "@/model/connections"
import { edgeText } from "@/model/evidence"
import {
  eventsFor,
  podHealth,
  podReason,
  serviceId,
  statusOf,
  type Event,
  type Service,
} from "@/model/service"

export function QuickLook({
  project,
  service,
  services,
  edges,
  events,
  onClose,
  onSelect,
}: {
  project: string
  service: Service
  services: Service[]
  edges: Edge[]
  events: Event[]
  onClose: () => void
  onSelect: (id: string) => void
}) {
  const t = useT()
  const { upstream, downstream } = neighboursOf(edges, service)
  const mine = eventsFor(events, service)

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !document.querySelector("[role=dialog]")) onClose()
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [onClose])

  const row = (edge: Edge, direction: "in" | "out") => {
    const otherId = direction === "out" ? edge.to : edge.from
    const other = services.find((s) => serviceId(s) === otherId)
    const DirectionIcon = direction === "out" ? ArrowRightIcon : ArrowLeftIcon
    return (
      <li key={edge.id} className="flex items-center gap-2 py-1.5 text-sm">
        <DirectionIcon className="size-3.5 shrink-0 text-muted-foreground" aria-hidden="true" />
        {other ? (
          <button
            type="button"
            onClick={() => onSelect(otherId)}
            className="flex min-w-0 items-center gap-1.5 rounded font-medium outline-none hover:underline focus-visible:ring-2 focus-visible:ring-ring"
            translate="no"
          >
            <StatusDot health={statusOf(other).health} />
            <span className="truncate">{other.name}</span>
          </button>
        ) : (
          <span className="flex min-w-0 items-center gap-1.5 font-mono text-xs" translate="no">
            <GlobeIcon className="size-3.5 shrink-0 text-muted-foreground" aria-hidden="true" />
            <span className="truncate">{otherId.replace(/^domain:/, "")}</span>
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
    <aside
      ref={focusOnMount}
      tabIndex={-1}
      aria-label={t("quickLook.label", service.name)}
      className="enter-from-right absolute inset-y-0 right-0 z-30 flex w-full flex-col overflow-y-auto border-l bg-background shadow-2xl outline-none sm:w-100"
    >
      <header className="flex flex-col gap-3 border-b p-4">
        <div className="flex items-start gap-3">
          <ServiceIcon service={service} size="lg" />
          <div className="flex min-w-0 flex-1 flex-col gap-1">
            <h2 className="truncate text-lg font-semibold" translate="no">
              {service.displayName ?? service.name}
            </h2>
            <span className="flex flex-wrap items-center gap-2">
              <ServiceStatus status={statusOf(service)} size="sm" counts />
              <span className="type-meta text-muted-foreground" translate="no">
                {service.kind}
              </span>
            </span>
          </div>
          <Button variant="ghost" size="icon-sm" onClick={onClose} aria-label={t("common.close")}>
            <XIcon aria-hidden="true" />
          </Button>
        </div>
        <Link {...serviceLink(project, service)} className={cn(buttonVariants({ size: "sm" }))}>
          <ArrowSquareOutIcon aria-hidden="true" data-icon="inline-start" />
          {t("quickLook.open")}
        </Link>
      </header>

      <section className="flex flex-col gap-1 border-b p-4" aria-labelledby="ql-pods">
        <h3 id="ql-pods" className="type-meta font-medium text-muted-foreground">
          {t("pod.pods")}
        </h3>
        {service.pods.length === 0 ? (
          <p className="text-sm text-muted-foreground">{t("pod.none")}</p>
        ) : (
          <ul className="flex flex-col">
            {service.pods.slice(0, 6).map((pod) => (
              <li key={pod.name} className="flex items-center gap-2 py-1 text-xs">
                <StatusDot health={podHealth(pod)} />
                <span className="min-w-0 truncate font-mono" translate="no">
                  {pod.name}
                </span>
                {!pod.ready ? (
                  <span className="shrink-0 text-fail">{podReason(pod) ?? t("pod.notReady")}</span>
                ) : null}
                <span className="ml-auto shrink-0 text-muted-foreground tabular-nums">
                  ↻ {pod.restarts}
                </span>
                <Link
                  {...podLogsLink(project, service, pod.name)}
                  className="shrink-0 rounded px-1.5 py-0.5 text-muted-foreground hover:bg-muted hover:text-foreground"
                >
                  {t("tab.logs")}
                </Link>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="flex flex-col gap-1 border-b p-4" aria-labelledby="ql-up">
        <h3 id="ql-up" className="type-meta font-medium text-muted-foreground">
          {t("quickLook.upstream")}
        </h3>
        {upstream.length === 0 ? (
          <p className="text-sm text-muted-foreground">—</p>
        ) : (
          <ul className="flex flex-col">{upstream.map((e) => row(e, "in"))}</ul>
        )}
      </section>
      <section className="flex flex-col gap-1 border-b p-4" aria-labelledby="ql-down">
        <h3 id="ql-down" className="type-meta font-medium text-muted-foreground">
          {t("quickLook.downstream")}
        </h3>
        {downstream.length === 0 ? (
          <p className="text-sm text-muted-foreground">—</p>
        ) : (
          <ul className="flex flex-col">{downstream.map((e) => row(e, "out"))}</ul>
        )}
      </section>

      <section className="flex flex-col gap-1 p-4" aria-labelledby="ql-events">
        <h3 id="ql-events" className="type-meta font-medium text-muted-foreground">
          {t("project.events")}
        </h3>
        <EventList events={mine} limit={4} compact />
      </section>
    </aside>
  )
}

/** Moves focus into the panel when it opens. Declared outside the component, so React calls it
 * when the panel mounts, not on every render. */
function focusOnMount(element: HTMLElement | null) {
  element?.focus()
}
