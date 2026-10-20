import { useLocation } from "@tanstack/react-router"
import { lazy, Suspense, useEffect, useState, type ReactNode } from "react"

import type { Schemas } from "@/api/client"
import { LoadingRows } from "@/components/page"
import { useDraft } from "@/components/plan/draft"
import { refusedAmong, RefusedKinds } from "@/components/refused"
import { SectionNav } from "@zeabur/design-system/components/section-nav"
import { useT } from "@/i18n"
import type { Edge } from "@/model/connections"
import { isTask, serviceId, type Network, type Service } from "@/model/service"
import { refusedPlaces } from "@/routes/project/data"
import type { ConfigurePart } from "@/routes/search"
import { NetworkingTab } from "@/routes/service/networking"
import { StorageTab } from "@/routes/service/storage"
import { VariablesTab } from "@/routes/service/variables"

const Compute = lazy(() =>
  import("@/routes/service/settings").then((m) => ({ default: m.ComputeSection })),
)
const HealthChecks = lazy(() =>
  import("@/routes/service/settings").then((m) => ({ default: m.HealthChecksSection })),
)

const partTitle: Record<ConfigurePart, `configure.${ConfigurePart}`> = {
  variables: "configure.variables",
  networking: "configure.networking",
  storage: "configure.storage",
  compute: "configure.compute",
  "health-checks": "configure.health-checks",
}

/** Every setting of the service on one page; edits wait in the page's draft until reviewed. */
export function ConfigureSection({
  project,
  service,
  services,
  network,
  edges,
  reachability,
  unreadable,
}: {
  project: string
  service: Service
  services: Service[]
  network: Network
  edges: Edge[]
  reachability?: Schemas["NetworkPolicyReachability"]
  unreadable: Schemas["UnreadableView"][]
}) {
  const t = useT()
  const draft = useDraft()
  const hash = useLocation({ select: (l) => l.hash })
  const parts: ConfigurePart[] = isTask(service.kind)
    ? ["variables", "networking", "storage", "compute"]
    : ["variables", "networking", "storage", "compute", "health-checks"]
  const [current, setCurrent] = useState<string>(
    parts.includes(hash as ConfigurePart) ? hash : parts[0]!,
  )
  const managed = service.provenance.tool

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
          aria-label={t("configure.sections")}
          items={parts.map((part) => ({
            id: part,
            label: t(partTitle[part]),
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
        {managed ? (
          <p className="rounded-lg border border-dashed px-3 py-2 text-sm text-muted-foreground">
            {t("common.managedByDetail", managed)}
          </p>
        ) : null}
        <Part id="variables">
          <VariablesTab
            project={project}
            service={service}
            edges={edges.filter((e) => e.from === serviceId(service))}
            refused={refusedAmong(unreadable, refusedPlaces.config)}
          />
        </Part>
        <Part id="networking">
          <NetworkingTab
            project={project}
            service={service}
            services={services}
            network={network}
            reachability={reachability}
            unreadable={unreadable}
          />
        </Part>
        <Part id="storage">
          <div className="flex flex-col gap-4">
            <RefusedKinds unreadable={refusedAmong(unreadable, refusedPlaces.claims)} />
            <StorageTab project={project} service={service} />
          </div>
        </Part>
        <Suspense fallback={<LoadingRows label={t("common.loading")} rows={4} />}>
          <Part id="compute">
            <Compute project={project} service={service} />
          </Part>
          {parts.includes("health-checks") ? (
            <Part id="health-checks">
              <HealthChecks project={project} service={service} network={network} />
            </Part>
          ) : null}
        </Suspense>
      </div>
    </div>
  )
}

function Part({ id, children }: { id: ConfigurePart; children: ReactNode }) {
  const t = useT()
  return (
    <section id={id} aria-label={t(partTitle[id])} className="scroll-mt-14 lg:scroll-mt-6">
      {children}
    </section>
  )
}

function PendingMark() {
  const t = useT()
  return (
    <span className="flex items-center">
      <span aria-hidden="true" className="size-1.5 rounded-full bg-progress" />
      <span className="sr-only">{t("draft.pendingHere")}</span>
    </span>
  )
}
