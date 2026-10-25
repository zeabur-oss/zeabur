import { useState } from "react"

import type { Schemas } from "@/api/client"
import { FormField } from "@/components/form-field"
import { KubeDetails } from "@/components/kube-details"
import { PlanDrawer, ReviewFooter, ReviewForm } from "@/components/plan/plan-drawer"
import { usePlanStep } from "@/components/plan/planner"
import { useT } from "@/i18n"
import { egressPolicyName, isAddress, splitAddresses } from "@/model/ip-limits"
import type { Service } from "@/model/service"
import { Label } from "@zeabur/design-system/components/label"
import { Switch } from "@zeabur/design-system/components/switch"
import { Textarea } from "@zeabur/design-system/components/textarea"

type Change = Schemas["Change"]

/** Limit who can open a service's domains (`in`) or where its pods connect (`out`). */
export default function IpLimitsDrawer({
  project,
  service,
  kind,
  ingresses,
  current,
  same,
  limitedNow,
  onClose,
}: {
  project: string
  service: Service
  kind: "in" | "out"
  ingresses: Schemas["Route"][]
  /** The addresses now; absent while they load or when nothing limits yet. */
  current?: string[]
  /** Whether every Ingress has the addresses now. */
  same: boolean
  limitedNow: boolean
  onClose: () => void
}) {
  const t = useT()
  const step = usePlanStep(project, onClose)
  const [on, setOn] = useState(limitedNow)
  const [text, setText] = useState((current ?? []).join("\n"))
  const [tried, setTried] = useState(false)
  const cidrs = splitAddresses(text)
  const bad = cidrs.filter((c) => !isAddress(c))
  const workload = { kind: service.kind, name: service.name }
  const unchanged = on === limitedNow && (!on || (same && cidrs.join() === (current ?? []).join()))
  const problem = !on
    ? undefined
    : bad.length > 0
      ? t("ip.bad", bad.join(", "))
      : kind === "in" && cidrs.length === 0
        ? t("ip.needOne")
        : undefined
  const changes: Change[] =
    kind === "out"
      ? [{ op: "egressLimit", workload, enabled: on, cidrs: on ? cidrs : [] }]
      : ingresses
          .filter((r) => on || r.ipAllow)
          .map((r) => ({
            op: "ipAllow",
            route: { kind: "Ingress", name: r.name },
            enabled: on,
            cidrs: on ? cidrs : [],
          }))
  const review = () => {
    setTried(true)
    if (problem || unchanged) return
    step.plan({
      title: t(kind === "in" ? "ip.planIn" : "ip.planOut", service.name),
      service: workload,
      changes,
    })
  }
  return (
    <PlanDrawer
      step={step}
      onClose={onClose}
      title={t(kind === "in" ? "ip.in.drawer" : "ip.out.drawer", service.name)}
      description={t(kind === "in" ? "ip.in.note" : "ip.out.note")}
      footer={
        <ReviewFooter
          onCancel={onClose}
          onReview={review}
          note={tried && unchanged ? t("ip.nothing") : undefined}
        />
      }
    >
      <ReviewForm onReview={review}>
        <div className="flex items-center gap-2">
          <Switch id="ip-on" size="sm" checked={on} onCheckedChange={setOn} />
          <Label htmlFor="ip-on">{t(kind === "in" ? "ip.in.switch" : "ip.out.switch")}</Label>
        </div>
        {on ? (
          <FormField
            label={t("ip.addresses")}
            hint={t(kind === "in" ? "ip.in.hint" : "ip.out.hint")}
            errors={tried && problem ? [problem] : []}
          >
            <Textarea
              mono
              rows={5}
              value={text}
              placeholder={"203.0.113.0/24\n198.51.100.7"}
              spellCheck={false}
              onChange={(event) => setText(event.target.value)}
            />
          </FormField>
        ) : null}
        <KubeDetails
          facts={
            kind === "in"
              ? ingresses.map((r): [string, string] => [r.name, t("ip.kube.in", r.name)])
              : [[service.name, t("ip.kube.out", egressPolicyName(service.name))]]
          }
        />
      </ReviewForm>
    </PlanDrawer>
  )
}
