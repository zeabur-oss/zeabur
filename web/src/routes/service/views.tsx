import { lazy, Suspense, type ReactNode } from "react"
import BellIcon from "~icons/ph/bell"
import ChartBarIcon from "~icons/ph/chart-bar"
import CodeIcon from "~icons/ph/code"
import TerminalWindowIcon from "~icons/ph/terminal-window"
import XIcon from "~icons/ph/x"

import type { Schemas } from "@/api/client"
import { refusedAmong, RefusedKinds } from "@/components/refused"
import { Button } from "@zeabur/design-system/components/button"
import { Skeleton } from "@zeabur/design-system/components/skeleton"
import { useT } from "@/i18n"
import type { Event, Service } from "@/model/service"
import { refusedPlaces } from "@/routes/project/data"
import type { ServiceView } from "@/routes/search"
import { EventsTab } from "@/routes/service/events"
import { MetricsTab } from "@/routes/service/metrics"
import { YamlTab } from "@/routes/service/yaml"

const LogsTab = lazy(() => import("@/routes/service/logs").then((m) => ({ default: m.LogsTab })))
const TerminalTab = lazy(() =>
  import("@/routes/service/terminal").then((m) => ({ default: m.TerminalTab })),
)

export const viewIcon = {
  usage: ChartBarIcon,
  events: BellIcon,
  terminal: TerminalWindowIcon,
  yaml: CodeIcon,
} as const

export const viewLabel: Record<
  ServiceView,
  "tab.metrics" | "tab.events" | "tab.terminal" | "tab.yaml"
> = {
  usage: "tab.metrics",
  events: "tab.events",
  terminal: "tab.terminal",
  yaml: "tab.yaml",
}

const frame = (child: ReactNode) => (
  <Suspense fallback={<Skeleton variant="card" className="min-h-96 w-full flex-1" />}>
    {child}
  </Suspense>
)

export function Logs(props: {
  project: string
  service: Service
  pod?: string
  onPodChange: (pod: string) => void
}) {
  return <div className="flex min-h-0 flex-1 flex-col">{frame(<LogsTab {...props} />)}</div>
}

/** A view from the header's menu, under its own title, with the way back to the tabs. */
export function ServiceViewPanel({
  project,
  service,
  view,
  pod,
  events,
  unreadable,
  onPod,
  onClose,
}: {
  project: string
  service: Service
  view: ServiceView
  pod?: string
  events: Event[]
  unreadable: Schemas["UnreadableView"][]
  onPod: (pod?: string) => void
  onClose: () => void
}) {
  const t = useT()
  const Icon = viewIcon[view]
  const wide = view === "terminal"
  return (
    <section
      aria-label={t(viewLabel[view])}
      className={wide ? "flex min-h-0 flex-1 flex-col gap-3" : "flex flex-col gap-4"}
    >
      <div className="flex items-center gap-2">
        <Icon className="size-4 text-secondary" aria-hidden="true" />
        <h2 className="type-heading">{t(viewLabel[view])}</h2>
        <Button variant="ghost" size="sm" className="ml-auto" onClick={onClose}>
          <XIcon aria-hidden="true" data-icon="inline-start" />
          {t("common.close")}
        </Button>
      </div>
      {view === "terminal"
        ? frame(<TerminalTab project={project} service={service} pod={pod} onPodChange={onPod} />)
        : null}
      {view === "usage" ? <MetricsTab project={project} service={service} /> : null}
      {view === "yaml" ? <YamlTab project={project} service={service} /> : null}
      {view === "events" ? (
        <div className="flex flex-col gap-4">
          <RefusedKinds unreadable={refusedAmong(unreadable, refusedPlaces.events)} />
          <EventsTab events={events} pod={pod} onClearPod={() => onPod(undefined)} />
        </div>
      ) : null}
    </section>
  )
}
