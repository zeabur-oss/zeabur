import { useState } from "react"

import type { Schemas } from "@/api/client"
import { FormField } from "@/components/form-field"
import { PlanDrawer, ReviewFooter, ReviewForm } from "@/components/plan/plan-drawer"
import { usePlanStep } from "@/components/plan/planner"
import { useT, type Translate } from "@/i18n"
import {
  badAddresses,
  isUnchanged,
  limitChanges,
  problemOf,
  splitAddresses,
  type Kind,
  type Was,
} from "@/model/ip-limits"
import type { Service } from "@/model/service"
import { Label } from "@zeabur/design-system/components/label"
import { Switch } from "@zeabur/design-system/components/switch"
import { Textarea } from "@zeabur/design-system/components/textarea"

/** The drawer's words for one of the two limits. */
function wordsOf(t: Translate, kind: Kind, name: string) {
  const [drawer, note, label, hint, plan] =
    kind === "in"
      ? (["ip.in.drawer", "ip.in.note", "ip.in.switch", "ip.in.hint", "ip.planIn"] as const)
      : (["ip.out.drawer", "ip.out.note", "ip.out.switch", "ip.out.hint", "ip.planOut"] as const)
  return {
    title: t(drawer, name),
    note: t(note),
    label: t(label),
    hint: t(hint),
    plan: t(plan, name),
  }
}

function errorOf(t: Translate, problem: "bad" | "none" | undefined, cidrs: string[]) {
  if (!problem) return []
  return [problem === "none" ? t("ip.needOne") : t("ip.bad", badAddresses(cidrs).join(", "))]
}

/** Limit who can open a service's domains (`in`) or where its pods connect (`out`). */
export default function IpLimitsDrawer({
  project,
  service,
  kind,
  ingresses,
  was,
  onClose,
}: {
  project: string
  service: Service
  kind: Kind
  ingresses: Schemas.Route[]
  was: Was
  onClose: () => void
}) {
  const t = useT()
  const step = usePlanStep(project, onClose)
  const [on, setOn] = useState(was.on)
  const [text, setText] = useState(was.cidrs.join("\n"))
  const [tried, setTried] = useState(false)
  const words = wordsOf(t, kind, service.name)
  const cidrs = splitAddresses(text)
  const problem = problemOf(kind, on, cidrs)
  const unchanged = isUnchanged(was, on, cidrs)
  const review = () => {
    setTried(true)
    if (problem || unchanged) return
    const workload = { kind: service.kind, name: service.name }
    const routes = ingresses.map((r) => ({ name: r.name, limited: r.ipAllow != null }))
    step.plan({
      title: words.plan,
      service: workload,
      changes: limitChanges(kind, workload, routes, on, cidrs),
    })
  }
  return (
    <PlanDrawer
      step={step}
      onClose={onClose}
      title={words.title}
      description={words.note}
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
          <Label htmlFor="ip-on">{words.label}</Label>
        </div>
        {on ? (
          <FormField
            label={t("ip.addresses")}
            hint={words.hint}
            errors={tried ? errorOf(t, problem, cidrs) : []}
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
      </ReviewForm>
    </PlanDrawer>
  )
}
