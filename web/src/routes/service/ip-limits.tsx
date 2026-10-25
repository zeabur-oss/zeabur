import { useQueries, useQuery } from "@tanstack/react-query"
import { lazy, Suspense, useState, type ReactNode } from "react"
import SignInIcon from "~icons/ph/sign-in"
import SignOutIcon from "~icons/ph/sign-out"

import type { Schemas } from "@/api/client"
import { objectQuery } from "@/api/queries"
import { GateTip, useGate } from "@/components/gate"
import { useT } from "@/i18n"
import { blocksOf, egressPolicyName, sourceRangeOf } from "@/model/ip-limits"
import type { Service } from "@/model/service"
import { Button } from "@zeabur/design-system/components/button"
import { SectionHeader } from "@zeabur/design-system/components/section-header"

const IpLimitsDrawer = lazy(() => import("@/routes/service/ip-limits-drawer"))

type Route = Schemas["Route"]

const middleware = (name: string) => ({
  group: "traefik.io",
  version: "v1alpha1",
  resource: "middlewares",
  name,
})

const policy = (name: string) => ({
  group: "networking.k8s.io",
  version: "v1",
  resource: "networkpolicies",
  name,
})

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
  const domainGate = useGate(project, "changeDomains")
  const policyGate = useGate(project, "changeNetworkPolicies")
  const [editing, setEditing] = useState<"in" | "out">()
  const managed = !!service.provenance.tool
  const limited = ingresses.filter((r) => r.ipAllow)
  const ranges = useQueries({
    queries: limited.map((r) => objectQuery(project, middleware(r.ipAllow?.middleware ?? ""))),
  })
  const inSets = ranges.map((q) => (q.data ? sourceRangeOf(q.data.yaml) : undefined))
  const loaded = inSets.every((s) => s !== undefined)
  const first = inSets[0] ?? []
  const sameIn =
    loaded && limited.length === ingresses.length && inSets.every((s) => s?.join() === first.join())

  const policyName = egressPolicyName(service.name)
  const outLimited = reachability?.selectedBy.includes(policyName) ?? false
  const outQuery = useQuery({ ...objectQuery(project, policy(policyName)), enabled: outLimited })
  const outSet = outLimited && outQuery.data ? blocksOf(outQuery.data.yaml) : undefined

  const inText = (() => {
    if (limited.length === 0) return t("ip.in.anyone")
    if (!loaded) return t("ip.in.limited")
    return sameIn ? t("ip.in.only", t("common.list", first)) : t("ip.in.some")
  })()
  const outText = (() => {
    if (!outLimited) return reachability?.egressIsolated ? t("ip.out.rules") : t("ip.out.anywhere")
    if (!outSet) return t("ip.in.limited")
    return outSet.length === 0 ? t("ip.out.none") : t("ip.out.only", t("common.list", outSet))
  })()
  const noDomain = ingresses.length === 0
  const inNote = noDomain ? (gateway ? t("ip.in.gateway") : t("ip.in.noDomain")) : undefined

  return (
    <section className="flex min-w-0 flex-col gap-3" aria-labelledby="net-ip">
      <SectionHeader id="net-ip" title={t("ip.title")} description={t("ip.about")} />
      <ul className="flex flex-col divide-y divide-v2-border overflow-hidden rounded-xl border border-v2-border bg-card">
        <Row
          icon={<SignInIcon aria-hidden="true" />}
          title={t("ip.in.title")}
          action={
            <GateTip gate={domainGate}>
              <Button
                variant="outline"
                size="xs"
                disabled={noDomain || managed}
                aria-label={t("ip.editIn", service.name)}
                onClick={() => setEditing("in")}
              >
                {t("ip.edit")}
              </Button>
            </GateTip>
          }
        >
          {inNote ?? inText}
        </Row>
        {reachability ? (
          <Row
            icon={<SignOutIcon aria-hidden="true" />}
            title={t("ip.out.title")}
            action={
              <GateTip gate={policyGate}>
                <Button
                  variant="outline"
                  size="xs"
                  disabled={managed}
                  aria-label={t("ip.editOut", service.name)}
                  onClick={() => setEditing("out")}
                >
                  {t("ip.edit")}
                </Button>
              </GateTip>
            }
          >
            {outText}
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
            current={editing === "in" ? (limited.length > 0 ? first : undefined) : outSet}
            same={editing === "in" ? sameIn : true}
            limitedNow={editing === "in" ? limited.length > 0 : outLimited}
            onClose={() => setEditing(undefined)}
          />
        </Suspense>
      ) : null}
    </section>
  )
}
