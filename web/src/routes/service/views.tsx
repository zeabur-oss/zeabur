import { lazy, Suspense, type ReactNode } from "react"
import TerminalWindowIcon from "~icons/ph/terminal-window"
import TextAlignLeftIcon from "~icons/ph/text-align-left"

import type { Schemas } from "@/api/client"
import { Segmented } from "@/components/bits"
import { refusedAmong, RefusedKinds } from "@/components/refused"
import { Skeleton } from "@zeabur/design-system/components/skeleton"
import { useT } from "@/i18n"
import type { Event, Service } from "@/model/service"
import { refusedPlaces } from "@/routes/project/data"
import { serviceModes, type ServiceMode, type ServiceTab } from "@/routes/search"
import { EventsTab } from "@/routes/service/events"

const LogsTab = lazy(() => import("@/routes/service/logs").then((m) => ({ default: m.LogsTab })))
const TerminalTab = lazy(() =>
  import("@/routes/service/terminal").then((m) => ({ default: m.TerminalTab })),
)

const frame = (child: ReactNode) => (
  <Suspense fallback={<Skeleton variant="card" className="min-h-96 w-full flex-1" />}>
    {child}
  </Suspense>
)

type ModeTab = (typeof serviceModes)[ServiceMode]

const modeOf: Record<ModeTab, ServiceMode> = {
  logs: "terminal",
  deployments: "events",
  settings: "yaml",
}

/** A tab's two views: its own, and the one for people who know Kubernetes. */
export function ModeSwitch({
  tab,
  mode,
  onMode,
  children,
}: {
  tab: ModeTab
  mode?: ServiceMode
  onMode: (mode?: ServiceMode) => void
  children?: ReactNode
}) {
  const t = useT()
  const other = modeOf[tab]
  const icons = tab === "logs"
  return (
    <div className="flex flex-wrap items-center gap-3">
      <Segmented
        label={t("service.views")}
        value={mode === other ? other : tab}
        onChange={(value) => onMode(value === other ? other : undefined)}
        options={[
          {
            value: tab as ServiceTab | ServiceMode,
            label: (
              <>
                {icons ? <TextAlignLeftIcon aria-hidden="true" /> : null}
                {t(`tab.${tab}`)}
              </>
            ),
          },
          {
            value: other,
            label: (
              <>
                {icons ? <TerminalWindowIcon aria-hidden="true" /> : null}
                {t(`tab.${other}`)}
              </>
            ),
          },
        ]}
      />
      {children}
    </div>
  )
}

export function Logs({
  project,
  service,
  mode,
  pod,
  onPodChange,
}: {
  project: string
  service: Service
  mode?: ServiceMode
  pod?: string
  onPodChange: (pod?: string) => void
}) {
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {mode === "terminal"
        ? frame(
            <TerminalTab project={project} service={service} pod={pod} onPodChange={onPodChange} />,
          )
        : frame(
            <LogsTab project={project} service={service} pod={pod} onPodChange={onPodChange} />,
          )}
    </div>
  )
}

export function Events({
  events,
  pod,
  unreadable,
  onPod,
}: {
  events: Event[]
  pod?: string
  unreadable: Schemas["UnreadableView"][]
  onPod: (pod?: string) => void
}) {
  return (
    <div className="flex flex-col gap-4">
      <RefusedKinds unreadable={refusedAmong(unreadable, refusedPlaces.events)} />
      <EventsTab events={events} pod={pod} onClearPod={() => onPod(undefined)} />
    </div>
  )
}
