import { lazy, Suspense, useState, type ReactNode } from "react"
import PlusIcon from "~icons/ph/plus"
import XIcon from "~icons/ph/x"

import type { Schemas } from "@/api/client"
import { CopyButton } from "@/components/bits"
import { GateTip, useGate } from "@/components/gate"
import type { usePlanner } from "@/components/plan/planner"
import type { Gate } from "@/model/rights"
import { useT } from "@/i18n"
import { isOpened, outside, typeLabel } from "@/model/public-ports"
import type { Endpoint, OpenedPort, Service } from "@/model/service"
import { Badge } from "@zeabur-oss/design-system/components/badge"
import { Button } from "@zeabur-oss/design-system/components/button"
import { SectionHeader } from "@zeabur-oss/design-system/components/section-header"

const OpenPort = lazy(() => import("@/routes/service/open-port"))

type Port = Schemas.Port
type Kind = Schemas.PortAddressKind
type Close = (opens: OpenedPort) => void
type T = ReturnType<typeof useT>

const KINDS: readonly Kind[] = ["project", "cluster", "node", "loadBalancer"]

const kindLabel = (t: T, kind: Kind): string =>
  ({
    project: t("addr.project"),
    cluster: t("addr.cluster"),
    node: t("addr.node"),
    loadBalancer: t("addr.loadBalancer"),
  })[kind]

/** The Kubernetes Services of a service, each port with the addresses its Service type gives:
 * the project and the cluster for every Service, the nodes for a node port, and the load
 * balancer for a `LoadBalancer` Service. */
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
          {endpoints.map((e) => (
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
        {reachable ? <Badge variant="warn">{t("addr.outside")}</Badge> : null}
        {reachable && database ? <Badge variant="fail">{t("ports.databaseOpen")}</Badge> : null}
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
  const t = useT()
  const forwards = p.targetPort && p.targetPort !== String(p.port) ? ` → ${p.targetPort}` : ""
  return (
    <div className="flex min-w-0 flex-col gap-1 rounded-lg bg-muted/40 px-3 py-2">
      <span className="type-meta font-mono text-muted-foreground" translate="no">
        {`${p.port}/${p.protocol}${forwards}`}
      </span>
      <dl className="flex min-w-0 flex-col gap-1">
        {KINDS.map((kind) => (
          <AddressRow key={kind} kind={kind} port={p} pending={pendingText(t, kind, e, p)} />
        ))}
      </dl>
    </div>
  )
}

/** What a row says when the Service type gives the kind but the server lists no address: a
 * node port whose nodes are unknown, or a load balancer that has not answered yet. */
function pendingText(t: T, kind: Kind, e: Endpoint, p: Port): string | undefined {
  if (kind === "node" && p.nodePort) return t("addr.nodePortOnly", p.nodePort)
  if (kind === "loadBalancer" && e.type === "LoadBalancer") return t("ports.waiting", p.port)
  return undefined
}

/** One kind of address of a port. `pending` stands in when there is no address yet; without
 * either, the row is left out. */
function AddressRow({ kind, port, pending }: { kind: Kind; port: Port; pending?: string }) {
  const t = useT()
  const addresses = (port.addresses ?? []).filter((a) => a.kind === kind)
  if (addresses.length === 0 && !pending) return null
  return (
    <div className="flex min-w-0 flex-wrap items-baseline gap-x-3 gap-y-1">
      <dt className="w-36 shrink-0 type-meta text-muted-foreground">{kindLabel(t, kind)}</dt>
      <dd className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1">
        {addresses.length === 0 ? (
          <span className="type-meta text-muted-foreground">{pending}</span>
        ) : (
          addresses.map((a) => (
            <span key={a.address} className="flex min-w-0 items-center gap-1">
              <code className="truncate font-mono text-sm" translate="no" title={a.address}>
                {a.address}
              </code>
              <CopyButton value={a.address} />
            </span>
          ))
        )}
      </dd>
    </div>
  )
}
