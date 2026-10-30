import { lazy, Suspense, type ReactNode } from "react"

import type { Schemas } from "@/api/client"
import { Segmented } from "@/components/bits"
import { refusedAmong, RefusedKinds } from "@/components/refused"
import { Skeleton } from "@zeabur/design-system/components/skeleton"
import { useT } from "@/i18n"
import { openShell } from "@/lib/dock"
import { isTask, type Event, type Service } from "@/model/service"
import { refusedPlaces } from "@/routes/project/data"
import { serviceModes, type ServiceMode, type ServiceTab } from "@/routes/search"
import { EventsTab } from "@/routes/service/events"
import type { LogSource } from "@/routes/service/logs"

const LogsTab = lazy(() => import("@/routes/service/logs").then((m) => ({ default: m.LogsTab })))

const frame = (child: ReactNode) => (
  <Suspense fallback={<Skeleton variant="card" className="min-h-96 w-full flex-1" />}>
    {child}
  </Suspense>
)

type ModeTab = (typeof serviceModes)[ServiceMode]

const modeOf: Record<ModeTab, ServiceMode> = {
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
  return (
    <div className="flex flex-wrap items-center gap-3">
      <Segmented
        label={t("service.views")}
        value={mode === other ? other : tab}
        onChange={(value) => onMode(value === other ? other : undefined)}
        options={[
          {
            value: tab as ServiceTab | ServiceMode,
            label: t(`tab.${tab}`),
          },
          {
            value: other,
            label: t(`tab.${other}`),
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
  pod,
  onPodChange,
}: {
  project: string
  service: Service
  pod?: string
  onPodChange: (pod?: string) => void
}) {
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {frame(
        <LogsTab
          project={project}
          source={{
            name: service.name,
            pods: service.pods,
            containers: service.containers.map((c) => c.name),
            history: service,
            shell: isTask(service.kind) ? undefined : (name) => openShell(project, service, name),
          }}
          pod={pod}
          onPodChange={onPodChange}
        />,
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
  unreadable: Schemas.UnreadableView[]
  onPod: (pod?: string) => void
}) {
  return (
    <div className="flex flex-col gap-4">
      <RefusedKinds unreadable={refusedAmong(unreadable, refusedPlaces.events)} />
      <EventsTab events={events} pod={pod} onClearPod={() => onPod(undefined)} />
    </div>
  )
}

/** A database's members' logs: no log store history, and its own terminal. */
export function MemberLogs({
  project,
  source,
  pod,
  onPodChange,
}: {
  project: string
  source: LogSource
  pod?: string
  onPodChange: (pod?: string) => void
}) {
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {frame(<LogsTab project={project} source={source} pod={pod} onPodChange={onPodChange} />)}
    </div>
  )
}
