// Who may reach a service and where it may go, from the project's NetworkPolicy objects (the
// project document's `extensions.networkPolicy`, evaluated on the server). Whether the cluster's
// network plugin enforces the policies is not known, and the section says so. Ready-made policies
// (isolate, allow the ingress controller, allow one service) open a plan to read before applying.
import { useState } from "react"
import CaretDownIcon from "~icons/ph/caret-down"
import ShieldPlusIcon from "~icons/ph/shield-plus"

import type { Schemas } from "@/api/client"
import { networkPresetsQuery, queryClient } from "@/api/queries"
import { StatusPill } from "@/components/status"
import { Button } from "@engawahq/design-system/components/button"
import { Menu, MenuContent, MenuItem, MenuTrigger } from "@engawahq/design-system/components/menu"
import { useT } from "@/i18n"
import type { Health } from "@/model/service"
import type { Service } from "@/model/service"
import type { PlanAsk } from "@/components/plan/planner"
import { Section } from "@/routes/service/section"

type Access = Schemas["NetworkPolicyAccess"]

const health: Record<Access, Health> = { allowed: "ok", limited: "warn", blocked: "fail" }

function AccessPill({ access }: { access: Access }) {
  const t = useT()
  return (
    <StatusPill health={health[access]} size="sm">
      {t(`reach.${access}`)}
    </StatusPill>
  )
}

export function Reachability({
  project,
  service,
  services,
  reach,
  policies,
  plan,
}: {
  project: string
  service: Service
  services: Service[]
  reach: Schemas["NetworkPolicyService"]
  /** Every policy of the project. */
  policies: string[]
  plan: (ask: PlanAsk) => void
}) {
  const t = useT()
  const [problem, setProblem] = useState<string>()
  const name = (id: string) => services.find((s) => s.id === id)?.name ?? id.split("/").pop()
  const choose = async (kind: string, from?: string) => {
    setProblem(undefined)
    try {
      const presets = await queryClient.fetchQuery(
        networkPresetsQuery(project, `${service.kind}/${service.name}`),
      )
      const preset = presets.items.find((p) => p.kind === kind && (p.from ?? undefined) === from)
      if (!preset) return
      plan({
        title:
          kind === "isolate"
            ? t("reach.planIsolate", service.name)
            : kind === "allow-from-ingress-controller"
              ? t("reach.planAllowIngress", service.name)
              : t("reach.planAllowFrom", name(from ?? "") ?? "", service.name),
        changes: [{ op: "create", object: preset.object }],
        service: { kind: service.kind, name: service.name },
      })
    } catch (error) {
      setProblem(error instanceof Error ? error.message : String(error))
    }
  }
  const others = services.filter((s) => s.id !== service.id)
  const row =
    "flex flex-wrap items-center gap-x-3 gap-y-1.5 border-t border-rule py-2 first:border-t-0"

  return (
    <Section
      title={t("reach.title")}
      action={
        <Menu>
          <MenuTrigger render={<Button variant="outline" size="xs" />}>
            <ShieldPlusIcon aria-hidden="true" data-icon="inline-start" />
            {t("reach.addPolicy")}
            <CaretDownIcon aria-hidden="true" data-icon="inline-end" />
          </MenuTrigger>
          <MenuContent className="w-72">
            <MenuItem onClick={() => void choose("isolate")}>{t("reach.isolate")}</MenuItem>
            <MenuItem onClick={() => void choose("allow-from-ingress-controller")}>
              {t("reach.allowIngress")}
            </MenuItem>
            {others.map((other) => (
              <MenuItem key={other.id} onClick={() => void choose("allow-from-service", other.id)}>
                {t("reach.allowFrom", other.name)}
              </MenuItem>
            ))}
          </MenuContent>
        </Menu>
      }
    >
      <div className="rounded-lg border bg-card shadow-xs px-4 py-1 text-sm">
        <div className={row}>
          <span className="w-28 shrink-0 text-muted-foreground">{t("reach.policies")}</span>
          {reach.selectedBy.length === 0 ? (
            <span className="text-muted-foreground">
              {policies.length === 0 ? t("reach.noPolicies") : t("reach.noneSelect")}
            </span>
          ) : (
            reach.selectedBy.map((policy) => (
              <code key={policy} className="rounded bg-muted px-1 font-mono text-xs" translate="no">
                {policy}
              </code>
            ))
          )}
        </div>
        <div className={row}>
          <span className="w-28 shrink-0 text-muted-foreground">{t("reach.in")}</span>
          <span className="flex items-center gap-1.5">
            {t("reach.ingressController")}
            <AccessPill access={reach.fromIngressController} />
          </span>
          <span className="flex items-center gap-1.5">
            {t("reach.otherNamespaces")}
            <AccessPill access={reach.fromOtherNamespaces} />
          </span>
          {reach.fromServices.map((peer) => (
            <span key={peer.service} className="flex items-center gap-1.5">
              <span className="font-mono text-xs" translate="no">
                {name(peer.service)}
              </span>
              <AccessPill access={peer.access} />
            </span>
          ))}
        </div>
        <div className={row}>
          <span className="w-28 shrink-0 text-muted-foreground">{t("reach.out")}</span>
          <span className="flex items-center gap-1.5">
            {t("reach.dns")}
            <AccessPill access={reach.toDns} />
          </span>
          <span className="flex items-center gap-1.5">
            {t("reach.outside")}
            <AccessPill access={reach.toOutside} />
          </span>
        </div>
      </div>
      <p className="type-meta text-muted-foreground">{t("reach.cni")}</p>
      {problem ? <p className="type-meta text-fail">{problem}</p> : null}
    </Section>
  )
}
