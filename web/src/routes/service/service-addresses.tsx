import { lazy, Suspense, useState, type ReactNode } from "react"
import GlobeIcon from "~icons/ph/globe"
import PlusIcon from "~icons/ph/plus"
import XIcon from "~icons/ph/x"

import type { Schemas } from "@/api/client"
import { CopyButton } from "@/components/bits"
import { GateTip, useGate } from "@/components/gate"
import type { usePlanner } from "@/components/plan/planner"
import type { Gate } from "@/model/rights"
import { useT } from "@/i18n"
import {
  addressRows,
  outside,
  typeLabel,
  type AddressKind,
  type AddressRow as Row,
} from "@/model/port-addresses"
import { isOpened } from "@/model/public-ports"
import type { Endpoint, OpenedPort, Service } from "@/model/service"
import { Badge } from "@zeabur-oss/design-system/components/badge"
import { Button } from "@zeabur-oss/design-system/components/button"
import { SectionHeader } from "@zeabur-oss/design-system/components/section-header"

const OpenPort = lazy(() => import("@/routes/service/open-port"))

type Port = Schemas.Port
type Close = (opens: OpenedPort) => void
type T = ReturnType<typeof useT>

const kindLabel = (t: T, kind: AddressKind): string => t(`addr.${kind}`)

/** The Kubernetes Services of a service, each port with the addresses its Service type gives.
 * The Services the internet can reach come first. */
export function ServiceAddresses({
  project,
  service,
  endpoints,
  plan,
  refused,
  empty,
}: {
  project: string
  service: Service
  endpoints: Endpoint[]
  plan: ReturnType<typeof usePlanner>["plan"]
  refused: ReactNode
  empty: ReactNode
}) {
  const t = useT()
  const gate = useGate(project, "changeDomains")
  const [opening, setOpening] = useState(false)
  const opened = endpoints.filter(isOpened)
  const openable = endpoints.filter((e) => !outside(e) && e.clusterIP !== "None")
  const workload = { kind: service.kind, name: service.name }
  const managed = !!service.provenance.tool
  const close = ({ service: from, port }: OpenedPort) =>
    plan({
      title: t("ports.closeTitle", port, service.name),
      service: workload,
      changes: [{ op: "publicPort", workload, service: from, port, remove: true }],
    })
  return (
    <section className="flex min-w-0 flex-col gap-3" aria-labelledby="net-services">
      <SectionHeader
        id="net-services"
        title={t("addr.title")}
        description={t("addr.about")}
        action={
          <GateTip gate={gate}>
            <Button
              variant="outline"
              size="sm"
              disabled={openable.length === 0 || managed}
              onClick={() => setOpening(true)}
            >
              <PlusIcon aria-hidden="true" data-icon="inline-start" />
              {t("ports.open")}
            </Button>
          </GateTip>
        }
      />
      {refused}
      {endpoints.length === 0 ? (
        empty
      ) : (
        <ul className="flex flex-col divide-y divide-v2-border overflow-hidden rounded-xl border border-v2-border bg-card">
          {[...endpoints.filter(outside), ...endpoints.filter((e) => !outside(e))].map((e) => (
            <ServiceCard
              key={e.name}
              endpoint={e}
              database={!!service.dataStore}
              onClose={managed ? undefined : close}
              gate={gate}
            />
          ))}
        </ul>
      )}
      {opening ? (
        <Suspense fallback={null}>
          <OpenPort
            project={project}
            service={service}
            endpoints={openable}
            open={opened}
            onClose={() => setOpening(false)}
          />
        </Suspense>
      ) : null}
    </section>
  )
}

function ServiceCard({
  endpoint: e,
  database,
  onClose,
  gate,
}: {
  endpoint: Endpoint
  database: boolean
  onClose: Close | undefined
  gate: Gate
}) {
  const t = useT()
  const reachable = outside(e)
  const opens = e.opens
  return (
    <li className="flex min-w-0 flex-col gap-2 px-4 py-3">
      <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
        <span className="truncate font-mono text-sm font-medium" translate="no">
          {e.name}
        </span>
        <Badge variant="outline">{typeLabel(e)}</Badge>
        {reachable ? (
          <Badge variant={database ? "fail" : "warn"}>
            <GlobeIcon aria-hidden="true" data-icon="inline-start" />
            {database ? t("ports.databaseOpen") : t("addr.internet")}
          </Badge>
        ) : null}
        {opens ? (
          <span className="type-meta text-muted-foreground" translate="no">
            {t("addr.opens", `${opens.service}:${opens.port}`)}
          </span>
        ) : null}
        <span className="flex-1" />
        {opens && onClose ? <CloseButton endpoint={e} onClose={onClose} gate={gate} /> : null}
      </div>
      {e.ports.map((p) => (
        <PortAddresses key={`${p.port}/${p.protocol}`} endpoint={e} port={p} />
      ))}
      {e.ports.some((p) => p.nodePort) ? (
        <p className="type-meta text-muted-foreground">{t("addr.nodeHint")}</p>
      ) : null}
    </li>
  )
}

function CloseButton({
  endpoint: e,
  onClose,
  gate,
}: {
  endpoint: Endpoint
  onClose: Close
  gate: Gate
}) {
  const t = useT()
  const opens = e.opens
  if (!opens) return null
  return (
    <GateTip gate={gate}>
      <Button
        variant="ghost"
        size="xs"
        onClick={() => onClose(opens)}
        aria-label={t("ports.close", e.ports[0]?.port ?? opens.port)}
      >
        <XIcon aria-hidden="true" data-icon="inline-start" />
        {t("ports.closeShort")}
      </Button>
    </GateTip>
  )
}

function PortAddresses({ endpoint: e, port: p }: { endpoint: Endpoint; port: Port }) {
  const forwards = p.targetPort && p.targetPort !== String(p.port) ? ` → ${p.targetPort}` : ""
  return (
    <div className="flex min-w-0 flex-col gap-1 rounded-lg bg-muted/40 px-3 py-2">
      <span className="type-meta font-mono text-muted-foreground" translate="no">
        {`${p.port}/${p.protocol}${forwards}`}
      </span>
      <dl className="flex min-w-0 flex-col gap-1">
        {addressRows(e, p).map((row) => (
          <AddressRow key={row.kind} row={row} />
        ))}
      </dl>
    </div>
  )
}

/** One kind of address of a port, or what stands in while it has none. */
function AddressRow({ row }: { row: Row }) {
  const t = useT()
  return (
    <div className="flex min-w-0 flex-wrap items-baseline gap-x-3 gap-y-1">
      <dt className="w-36 shrink-0 type-meta text-muted-foreground">{kindLabel(t, row.kind)}</dt>
      <dd className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1">
        {row.pending ? (
          <span className="type-meta text-muted-foreground">
            {t(row.pending.message, row.pending.port)}
          </span>
        ) : (
          row.addresses.map((a) => (
            <span key={a} className="flex min-w-0 items-center gap-1">
              <code className="truncate font-mono text-sm" translate="no" title={a}>
                {a}
              </code>
              <CopyButton value={a} />
            </span>
          ))
        )}
      </dd>
    </div>
  )
}
