import { useQueries, useQuery } from "@tanstack/react-query"
import { lazy, Suspense, useState, type ReactNode } from "react"
import SignInIcon from "~icons/ph/sign-in"
import SignOutIcon from "~icons/ph/sign-out"

import type { Schemas } from "@/api/client"
import { objectQuery } from "@/api/queries"
import { GateTip, useGate } from "@/components/gate"
import { useT, type Translate } from "@/i18n"
import {
  blocksOf,
  egressPolicyName,
  incomingVerdict,
  outgoingVerdict,
  sourceRangeOf,
  type Kind,
  type Verdict,
} from "@/model/ip-limits"
import type { Service } from "@/model/service"
import { Button } from "@zeabur/design-system/components/button"
import { SectionHeader } from "@zeabur/design-system/components/section-header"

const IpLimitsDrawer = lazy(() => import("@/routes/service/ip-limits-drawer"))

type Route = Schemas["Route"]

const address = (group: string, version: string, resource: string, name: string) => ({
  group,
  version,
  resource,
  name,
})

/** What a row says; `none` is the pods' limit with no address besides the project and DNS. */
function say(t: Translate, kind: Kind, verdict: Verdict): string {
  if (verdict.is !== "only") return t(`ip.${verdict.is}`)
  if (verdict.addresses.length === 0) return t("ip.out.none")
  const list = t("common.list", verdict.addresses)
  return t(kind === "in" ? "ip.in.only" : "ip.out.only", list)
}

function Row({
  icon,
  title,
  children,
  action,
}: {
  icon: ReactNode
  title: string
  children: ReactNode
  action: ReactNode
}) {
  return (
    <li className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1.5 px-4 py-3">
      <span className="text-base text-muted-foreground">{icon}</span>
      <span className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="text-sm font-medium">{title}</span>
        <span className="type-meta break-words text-muted-foreground">{children}</span>
      </span>
      {action}
    </li>
  )
}

/** The addresses each limited Ingress lets through, once read. */
function useIncoming(project: string, ingresses: Route[]) {
  const limited = ingresses.filter((r) => r.ipAllow)
  const ranges = useQueries({
    queries: limited.map((r) =>
      objectQuery(
        project,
        address("traefik.io", "v1alpha1", "middlewares", r.ipAllow?.middleware ?? ""),
      ),
    ),
  })
  const loaded = ranges.map((q) => (q.data ? sourceRangeOf(q.data.yaml) : undefined))
  const verdict = incomingVerdict(ingresses.length, loaded)
  const was = { on: limited.length > 0, same: verdict.is === "only", cidrs: loaded[0] ?? [] }
  return { limited, verdict, was }
}

/** The ranges of the pods' egress policy, once read. */
function useOutgoing(project: string, service: Service, reach?: Schemas["NetworkPolicyService"]) {
  const name = egressPolicyName(service.name)
  const ours = reach?.selectedBy.includes(name) ?? false
  const found = useQuery({
    ...objectQuery(project, address("networking.k8s.io", "v1", "networkpolicies", name)),
    enabled: ours,
  })
  const blocks = found.data ? blocksOf(found.data.yaml) : undefined
  const verdict = outgoingVerdict(ours, reach?.egressIsolated ?? false, blocks)
  return { ours, verdict, was: { on: ours, same: true, cidrs: blocks ?? [] } }
}

/** Which addresses may open a service's domains, and which its pods may connect to. */
export function IpLimits({
  project,
  service,
  ingresses,
  gateway,
  reachability,
}: {
  project: string
  service: Service
  /** The Ingresses of the service that Traefik serves. */
  ingresses: Route[]
  /** Whether the service also has a domain on a Gateway, which cannot be limited. */
  gateway: boolean
  reachability?: Schemas["NetworkPolicyService"]
}) {
  const t = useT()
  const [editing, setEditing] = useState<Kind>()
  const domains = useGate(project, "changeDomains")
  const policies = useGate(project, "changeNetworkPolicies")
  const incoming = useIncoming(project, ingresses)
  const outgoing = useOutgoing(project, service, reachability)
  const managed = !!service.provenance.tool
  const none = ingresses.length === 0
  const edit = (kind: Kind, disabled: boolean) => (
    <Button
      variant="outline"
      size="xs"
      disabled={disabled || managed}
      aria-label={t(kind === "in" ? "ip.editIn" : "ip.editOut", service.name)}
      onClick={() => setEditing(kind)}
    >
      {t("ip.edit")}
    </Button>
  )
  return (
    <section className="flex min-w-0 flex-col gap-3" aria-labelledby="net-ip">
      <SectionHeader id="net-ip" title={t("ip.title")} description={t("ip.about")} />
      <ul className="flex flex-col divide-y divide-v2-border overflow-hidden rounded-xl border border-v2-border bg-card">
        <Row
          icon={<SignInIcon aria-hidden="true" />}
          title={t("ip.in.title")}
          action={<GateTip gate={domains}>{edit("in", none)}</GateTip>}
        >
          {none ? t(gateway ? "ip.in.gateway" : "ip.in.noDomain") : say(t, "in", incoming.verdict)}
        </Row>
        {reachability ? (
          <Row
            icon={<SignOutIcon aria-hidden="true" />}
            title={t("ip.out.title")}
            action={<GateTip gate={policies}>{edit("out", false)}</GateTip>}
          >
            {say(t, "out", outgoing.verdict)}
          </Row>
        ) : null}
      </ul>
      {editing ? (
        <Suspense fallback={null}>
          <IpLimitsDrawer
            project={project}
            service={service}
            kind={editing}
            ingresses={ingresses}
            was={(editing === "in" ? incoming : outgoing).was}
            onClose={() => setEditing(undefined)}
          />
        </Suspense>
      ) : null}
    </section>
  )
}
