import { useLocation } from "@tanstack/react-router"
import { lazy, Suspense, useEffect, type ReactNode } from "react"

import type { Schemas } from "@/api/client"
import { LoadingRows } from "@/components/page"
import { useDraft } from "@/components/plan/draft"
import { refusedAmong, RefusedKinds } from "@/components/refused"
import { useT } from "@/i18n"
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
const HealthChecks = lazy(() =>
  import("@/routes/service/settings").then((m) => ({
    default: m.HealthChecksSection,
  })),
)

/** Compute, health checks and storage, one under the other; edits wait in the draft. */
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
  unreadable: Schemas["UnreadableView"][]
  mode?: ServiceMode
  onMode: (mode?: ServiceMode) => void
}) {
  const t = useT()
  const draft = useDraft()
  const hash = useLocation({ select: (l) => l.hash })
  const parts: SettingsPart[] = isTask(service.kind)
    ? ["compute", "storage"]
    : ["compute", "health-checks", "storage"]

  useEffect(() => {
    if (!hash || mode) return
    document.getElementById(hash)?.scrollIntoView({ block: "start" })
  }, [hash, mode])

  return (
    <div className="flex flex-col gap-6">
      <ModeSwitch tab="settings" mode={mode} onMode={onMode}>
        {mode ? null : (
          <nav
            aria-label={t("settings.sections")}
            className="flex flex-wrap items-center gap-x-4 gap-y-1"
          >
            {parts.map((part) => (
              <a
                key={part}
                href={`#${part}`}
                onClick={(event) => {
                  event.preventDefault()
                  document
                    .getElementById(part)
                    ?.scrollIntoView({ block: "start", behavior: "smooth" })
                  history.replaceState(history.state, "", `#${part}`)
                }}
                className="flex items-center gap-1.5 type-meta text-secondary outline-none hover:text-main focus-visible:underline"
              >
                {t(`part.${part}`)}
                {draft?.parts.includes(part) ? <PendingMark /> : null}
              </a>
            ))}
          </nav>
        )}
      </ModeSwitch>
      {mode === "yaml" ? <YamlTab project={project} service={service} /> : null}
      {/* Kept while the YAML shows, so edits typed here stay in the draft. */}
      <div key={draft?.generation} hidden={mode === "yaml"} className="flex min-w-0 flex-col gap-8">
        <Suspense fallback={<LoadingRows label={t("common.loading")} rows={4} />}>
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
        </Suspense>
      </div>
    </div>
  )
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
export function PendingMark() {
  const t = useT()
  return (
    <span className="flex items-center">
      <span aria-hidden="true" className="size-1.5 rounded-full bg-progress" />
      <span className="sr-only">{t("draft.pendingHere")}</span>
    </span>
  )
}
