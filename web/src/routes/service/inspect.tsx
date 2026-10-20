import { lazy, Suspense, type ReactNode } from "react"
import BellIcon from "~icons/ph/bell"
import ChartBarIcon from "~icons/ph/chart-bar"
import CodeIcon from "~icons/ph/code"
import TerminalWindowIcon from "~icons/ph/terminal-window"
import TextAlignLeftIcon from "~icons/ph/text-align-left"

import type { Schemas } from "@/api/client"
import { Segmented } from "@/components/bits"
import { refusedAmong, RefusedKinds } from "@/components/refused"
import { Skeleton } from "@zeabur/design-system/components/skeleton"
import { useT } from "@/i18n"
import type { Event, Service } from "@/model/service"
import { refusedPlaces } from "@/routes/project/data"
import type { InspectView } from "@/routes/search"
import { EventsTab } from "@/routes/service/events"
import { MetricsTab } from "@/routes/service/metrics"
import { YamlTab } from "@/routes/service/yaml"

const LogsTab = lazy(() => import("@/routes/service/logs").then((m) => ({ default: m.LogsTab })))
const TerminalTab = lazy(() =>
  import("@/routes/service/terminal").then((m) => ({ default: m.TerminalTab })),
)

const viewIcon = {
  logs: TextAlignLeftIcon,
  terminal: TerminalWindowIcon,
  usage: ChartBarIcon,
  yaml: CodeIcon,
  events: BellIcon,
} as const

const viewLabel: Record<
  InspectView,
  "tab.logs" | "tab.terminal" | "tab.metrics" | "tab.yaml" | "tab.events"
> = {
  logs: "tab.logs",
  terminal: "tab.terminal",
  usage: "tab.metrics",
  yaml: "tab.yaml",
  events: "tab.events",
}

/** The raw views of a running service: logs, a shell, usage, its YAML and its events. */
export function InspectSection({
  project,
  service,
  view,
  pod,
  events,
  unreadable,
  onView,
  onPod,
}: {
  project: string
  service: Service
  view: InspectView
  pod?: string
  events: Event[]
  unreadable: Schemas["UnreadableView"][]
  onView: (view: InspectView) => void
  onPod: (pod?: string) => void
}) {
  const t = useT()
  const views: InspectView[] = ["logs", "terminal", "usage", "yaml", "events"]
  const wide = view === "logs" || view === "terminal"
  const frame = (child: ReactNode) => (
    <Suspense fallback={<Skeleton variant="card" className="min-h-96 w-full flex-1" />}>
      {child}
    </Suspense>
  )
  return (
    <div className={wide ? "flex min-h-0 flex-1 flex-col gap-3" : "flex flex-col gap-4"}>
      <div className="flex min-w-0 overflow-x-auto">
        <Segmented
          label={t("inspect.views")}
          value={view}
          onChange={onView}
          options={views.map((value) => {
            const Icon = viewIcon[value]
            return {
              value,
              label: (
                <>
                  <Icon aria-hidden="true" />
                  {t(viewLabel[value])}
                  {value === "events" && events.length > 0 ? (
                    <span className="tabular-nums opacity-60">{events.length}</span>
                  ) : null}
                </>
              ),
            }
          })}
        />
      </div>
      {view === "logs"
        ? frame(<LogsTab project={project} service={service} pod={pod} onPodChange={onPod} />)
        : null}
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
    </div>
  )
}
