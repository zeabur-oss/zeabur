import { useLocation } from "@tanstack/react-router"
import { lazy, Suspense, useEffect, useState, type ReactNode } from "react"

import type { Schemas } from "@/api/client"
import { LoadingRows } from "@/components/page"
import { useDraft } from "@/components/plan/draft"
import { refusedAmong, RefusedKinds } from "@/components/refused"
import { SectionNav } from "@zeabur/design-system/components/section-nav"
import { useT } from "@/i18n"
import { isTask, type Network, type Service } from "@/model/service"
import { refusedPlaces } from "@/routes/project/data"
import type { SettingsPart } from "@/routes/search"
import { StorageTab } from "@/routes/service/storage"

const Compute = lazy(() =>
  import("@/routes/service/settings").then((m) => ({ default: m.ComputeSection })),
)
const HealthChecks = lazy(() =>
  import("@/routes/service/settings").then((m) => ({ default: m.HealthChecksSection })),
)

/** Compute, health checks and storage on one page beside their nav; edits wait in the draft. */
export function SettingsTab({
  project,
  service,
  network,
  unreadable,
}: {
  project: string
  service: Service
  network: Network
  unreadable: Schemas["UnreadableView"][]
}) {
  const t = useT()
  const draft = useDraft()
  const hash = useLocation({ select: (l) => l.hash })
  const parts: SettingsPart[] = isTask(service.kind)
    ? ["compute", "storage"]
    : ["compute", "health-checks", "storage"]
  const [current, setCurrent] = useState<string>(
    parts.includes(hash as SettingsPart) ? hash : parts[0]!,
  )

  useEffect(() => {
    if (!hash) return
    const target = document.getElementById(hash)
    if (!target) return
    target.scrollIntoView({ block: "start" })
    setCurrent(hash)
  }, [hash])

  useEffect(() => {
    const observer = new IntersectionObserver(
      (entries) => {
        const seen = entries.filter((e) => e.isIntersecting).map((e) => e.target.id)
        const first = parts.find((p) => seen.includes(p))
        if (first) setCurrent(first)
      },
      { rootMargin: "0px 0px -70% 0px" },
    )
    for (const part of parts) {
      const element = document.getElementById(part)
      if (element) observer.observe(element)
    }
    return () => observer.disconnect()
    // oxlint-disable-next-line react-hooks/exhaustive-deps -- the parts follow the service's kind, fixed per mount
  }, [])

  return (
    <div className="flex flex-col gap-6 lg:flex-row lg:items-start lg:gap-10">
      <div className="sticky top-0 z-10 -mx-1 bg-surface-page py-1 lg:top-6 lg:w-44 lg:shrink-0 lg:py-0">
        <SectionNav
          aria-label={t("settings.sections")}
          items={parts.map((part) => ({
            id: part,
            label: t(`part.${part}`),
            trailing: draft?.parts.includes(part) ? <PendingMark /> : undefined,
          }))}
          current={current}
          onSelect={(id, event) => {
            event.preventDefault()
            document.getElementById(id)?.scrollIntoView({ block: "start", behavior: "smooth" })
            history.replaceState(history.state, "", `#${id}`)
            setCurrent(id)
          }}
        />
      </div>
      <div key={draft?.generation} className="flex min-w-0 flex-1 flex-col gap-14">
        <Managed tool={service.provenance.tool} />
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

/** The tool that owns the service, above a tab whose edits it would undo. */
export function Managed({ tool }: { tool?: string | null }) {
  const t = useT()
  if (!tool) return null
  return (
    <p className="rounded-lg border border-dashed px-3 py-2 text-sm text-muted-foreground">
      {t("common.managedByDetail", tool)}
    </p>
  )
}

function Part({ id, children }: { id: SettingsPart; children: ReactNode }) {
  const t = useT()
  return (
    <section id={id} aria-label={t(`part.${id}`)} className="scroll-mt-14 lg:scroll-mt-6">
      {children}
    </section>
  )
}

/** A pending edit's mark, beside a part in a nav or a tab. */
export function PendingMark() {
  const t = useT()
  return (
    <span className="flex items-center">
      <span aria-hidden="true" className="size-1.5 rounded-full bg-progress" />
      <span className="sr-only">{t("draft.pendingHere")}</span>
    </span>
  )
}
