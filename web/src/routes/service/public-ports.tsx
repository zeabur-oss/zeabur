import { lazy, Suspense, useState } from "react"
import PlusIcon from "~icons/ph/plus"
import XIcon from "~icons/ph/x"

import { CopyButton } from "@/components/bits"
import { GateTip, useGate } from "@/components/gate"
import { KubeDetails } from "@/components/kube-details"
import type { usePlanner } from "@/components/plan/planner"
import { useT } from "@/i18n"
import type { Endpoint, Service } from "@/model/service"
import { Badge } from "@zeabur/design-system/components/badge"
import { Button } from "@zeabur/design-system/components/button"
import { SectionHeader } from "@zeabur/design-system/components/section-header"

const OpenPort = lazy(() => import("@/routes/service/open-port"))

/** The `LoadBalancer` Service Zeabur makes for a port: `<service>-public-<port>`. */
const made = /^(.+)-public-(\d+)$/

/** The ports of a service that anyone on the internet can connect to: its `LoadBalancer`
 * Services. */
export function PublicPorts({
  project,
  service,
  endpoints,
  plan,
}: {
  project: string
  service: Service
  endpoints: Endpoint[]
  plan: ReturnType<typeof usePlanner>["plan"]
}) {
  const t = useT()
  const gate = useGate(project, "changeDomains")
  const [opening, setOpening] = useState(false)
  const open = endpoints.filter((e) => e.type === "LoadBalancer")
  const closed = endpoints.filter((e) => e.type !== "LoadBalancer" && e.clusterIP !== "None")
  const workload = { kind: service.kind, name: service.name }
  const managed = !!service.provenance.tool
  const close = (name: string, port: number) => {
    const from = made.exec(name)?.[1]
    if (!from) return
    plan({
      title: t("ports.closeTitle", port, service.name),
      service: workload,
      changes: [{ op: "publicPort", workload, service: from, port, remove: true }],
    })
  }
  return (
    <section className="flex min-w-0 flex-col gap-3" aria-labelledby="net-ports">
      <SectionHeader
        id="net-ports"
        title={t("ports.title")}
        description={t("ports.about")}
        action={
          <GateTip gate={gate}>
            <Button
              variant="outline"
              size="sm"
              disabled={closed.length === 0 || managed}
              onClick={() => setOpening(true)}
            >
              <PlusIcon aria-hidden="true" data-icon="inline-start" />
              {t("ports.open")}
            </Button>
          </GateTip>
        }
      />
      {open.length === 0 ? (
        <p className="type-meta text-muted-foreground">{t("ports.none")}</p>
      ) : (
        <ul className="flex flex-col divide-y divide-v2-border overflow-hidden rounded-xl border border-v2-border bg-card">
          {open.flatMap((e) =>
            e.ports.map((p) => {
              const addresses = e.externalAddresses ?? []
              return (
                <li
                  key={`${e.name}/${p.port}`}
                  className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1.5 px-4 py-3"
                >
                  <span className="flex min-w-0 flex-1 flex-wrap items-center gap-x-3 gap-y-1">
                    {addresses.length === 0 ? (
                      <span className="type-meta text-muted-foreground">
                        {t("ports.waiting", p.port)}
                      </span>
                    ) : (
                      addresses.map((a) => (
                        <span key={a} className="flex min-w-0 items-center gap-1">
                          <span className="truncate font-mono text-sm font-medium" translate="no">
                            {`${a}:${p.port}`}
                          </span>
                          <CopyButton value={`${a}:${p.port}`} />
                        </span>
                      ))
                    )}
                    <span className="type-meta text-muted-foreground" translate="no">
                      {p.protocol}
                    </span>
                    {service.dataStore ? (
                      <Badge variant="fail">{t("ports.databaseOpen")}</Badge>
                    ) : null}
                  </span>
                  {made.test(e.name) && !managed ? (
                    <GateTip gate={gate}>
                      <Button
                        variant="ghost"
                        size="xs"
                        onClick={() => close(e.name, p.port)}
                        aria-label={t("ports.close", p.port)}
                      >
                        <XIcon aria-hidden="true" data-icon="inline-start" />
                        {t("ports.closeShort")}
                      </Button>
                    </GateTip>
                  ) : null}
                </li>
              )
            }),
          )}
        </ul>
      )}
      <KubeDetails
        facts={[
          ...open.map((e): [string, string] => [e.name, `Service/${e.name} LoadBalancer`]),
          [t("ports.kube.how"), t("ports.kube.howValue")],
        ]}
      />
      {opening ? (
        <Suspense fallback={null}>
          <OpenPort
            project={project}
            service={service}
            endpoints={closed}
            open={open}
            onClose={() => setOpening(false)}
          />
        </Suspense>
      ) : null}
    </section>
  )
}
