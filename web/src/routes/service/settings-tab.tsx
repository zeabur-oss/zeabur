import { useLocation } from "@tanstack/react-router"
import { lazy, Suspense, useEffect, type ReactNode } from "react"

import { useQuery } from "@tanstack/react-query"

import type { Schemas } from "@/api/client"
import { buildsQuery } from "@/api/queries"
import { PartNav } from "@/components/part-nav"
import { LoadingRows } from "@/components/page"
import { cn } from "@/lib/utils"
import { useDraft } from "@/components/plan/draft"
import { refusedAmong, RefusedKinds } from "@/components/refused"
import { useT } from "@/i18n"
import { buildsOf } from "@/model/builds"
import { isTask, type Network, type Service } from "@/model/service"
import { refusedPlaces } from "@/routes/project/data"
import type { ServiceMode, SettingsPart } from "@/routes/search"
import { StorageTab } from "@/routes/service/storage"
import { ModeSwitch } from "@/routes/service/views"
import { YamlTab } from "@/routes/service/yaml"

const Compute = lazy(() =>
  import("@/routes/service/settings").then((m) => ({
    default: m.ComputeSection,
  })),
)
const BuildSection = lazy(() =>
  import("@/routes/service/settings").then((m) => ({
    default: m.BuildSection,
  })),
)
const Security = lazy(() =>
  import("@/routes/service/security").then((m) => ({
    default: m.SecuritySection,
  })),
)
const Tasks = lazy(() =>
  import("@/routes/service/tasks").then((m) => ({
    default: m.TasksSection,
  })),
)
const ScheduleSection = lazy(() =>
  import("@/routes/service/tasks").then((m) => ({
    default: m.ScheduleSection,
  })),
)
const HealthChecks = lazy(() =>
  import("@/routes/service/settings").then((m) => ({
    default: m.HealthChecksSection,
  })),
)

/** Build, schedule, compute, health checks, storage, security and scheduled tasks, one under the other;
 * edits other than the build's wait in the draft. */
export function SettingsTab({
  project,
  service,
  network,
  unreadable,
  mode,
  onMode,
}: {
  project: string
  service: Service
  network: Network
  unreadable: Schemas.UnreadableView[]
  mode?: ServiceMode
  onMode: (mode?: ServiceMode) => void
}) {
  const t = useT()
  const draft = useDraft()
  const hash = useLocation({ select: (l) => l.hash })
  const builds = buildsOf(service, useQuery(buildsQuery(project)).data?.builds)
  const parts = partsOf(service, builds.length > 0)

  useEffect(() => {
    if (!hash || mode) return
    document.getElementById(hash)?.scrollIntoView({ block: "start" })
  }, [hash, mode])

  return (
    <div className="flex flex-col gap-6">
      <ModeSwitch tab="settings" mode={mode} onMode={onMode}>
        {mode ? null : (
          <PartNav
            label={t("settings.sections")}
            parts={parts}
            name={(part) => t(`part.${part}`)}
            mark={(part) => (draft?.parts.includes(part) ? <PendingMark /> : null)}
          />
        )}
      </ModeSwitch>
      {mode === "yaml" ? <YamlTab project={project} service={service} /> : null}
      {/* Kept while the YAML shows, so edits typed here stay in the draft. */}
      <div key={draft?.generation} hidden={mode === "yaml"} className="flex min-w-0 flex-col gap-8">
        <Suspense fallback={<LoadingRows label={t("common.loading")} rows={4} />}>
          {parts.includes("build") ? (
            <Part id="build">
              <BuildSection project={project} builds={builds} />
            </Part>
          ) : null}
          {parts.includes("schedule") ? (
            <Part id="schedule">
              <ScheduleSection project={project} service={service} />
            </Part>
          ) : null}
          <Part id="compute">
            <Compute project={project} service={service} />
          </Part>
          {parts.includes("health-checks") ? (
            <Part id="health-checks">
              <HealthChecks project={project} service={service} network={network} />
            </Part>
          ) : null}
          <Part id="storage">
            <div className="flex flex-col gap-4">
              <RefusedKinds unreadable={refusedAmong(unreadable, refusedPlaces.claims)} />
              <StorageTab project={project} service={service} />
            </div>
          </Part>
          {parts.includes("security") ? (
            <Part id="security">
              <Security project={project} service={service} />
            </Part>
          ) : null}
          {parts.includes("tasks") ? (
            <Part id="tasks">
              <Tasks project={project} service={service} />
            </Part>
          ) : null}
        </Suspense>
      </div>
    </div>
  )
}

function partsOf(service: Service, built: boolean): SettingsPart[] {
  return [
    ...(built ? (["build"] as const) : []),
    ...(service.schedule ? (["schedule"] as const) : []),
    "compute",
    ...(isTask(service.kind) ? [] : (["health-checks"] as const)),
    "storage",
    // A Job's pod template can't change.
    ...(service.kind === "Job" ? [] : (["security"] as const)),
    ...(isTask(service.kind) ? [] : (["tasks"] as const)),
  ]
}

function Part({ id, children }: { id: SettingsPart; children: ReactNode }) {
  const t = useT()
  return (
    <section id={id} aria-label={t(`part.${id}`)} className="scroll-mt-6">
      {children}
    </section>
  )
}

/** A pending edit's mark, beside a part's link or a tab. */
export function PendingMark({ className }: { className?: string }) {
  const t = useT()
  return (
    <span className={cn("flex items-center", className)}>
      <span aria-hidden="true" className="size-1.5 rounded-full bg-progress" />
      <span className="sr-only">{t("draft.pendingHere")}</span>
    </span>
  )
}
