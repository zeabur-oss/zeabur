// A toast's "Review again" uses the page's dialog while the page is mounted, else the shell's
// (`PlanFallback` in fallback.tsx).
import { lazy, Suspense, useEffect, useRef, useState } from "react"

import type { Problem, Schemas } from "@/api/client"
import { useDraft } from "@/components/plan/draft"
import { fallback, next, type Open } from "@/components/plan/fallback"
import type { Skipped } from "@/model/managed"

const PlanDialog = lazy(() =>
  import("@/components/plan/plan-dialog").then((m) => ({ default: m.PlanDialog })),
)

export interface PlanAsk {
  title: string
  changes: Schemas["Change"][]
  /** Also restart the services that read a changed ConfigMap or Secret into variables. */
  restartAfter?: boolean
  /** When the changes do not name it (a ConfigMap, a route, a policy). */
  service?: Schemas["WorkloadRef"]
  /** Services the changes leave out because another tool manages them. */
  skipped?: Skipped[]
  /** No toast once applied: what follows (a terminal opening) shows it. */
  quiet?: boolean
}

/** For a plan of objects written as YAML. */
export interface PlanHooks {
  /** Where in the YAML a refusal points, in words. */
  where?: (at: Schemas["ProblemAt"]) => string
  /** An apply refused after it created some of the objects. */
  onPartial?: (problem: Problem) => void
}

/** Inside a service's Configure page, an edit waits in its draft instead; `direct` plans at once. */
export function usePlanner(
  project: string,
  onApplied?: () => void,
  direct = false,
  hooks: PlanHooks = {},
) {
  const draft = useDraft()
  const [open, setOpen] = useState<Open | null>(null)
  const [refusal, setRefusal] = useState<Problem | undefined>(undefined)
  const mounted = useRef(false)
  useEffect(() => {
    mounted.current = true
    return () => {
      mounted.current = false
    }
  }, [])
  const show = (ask: PlanAsk) => {
    setRefusal(undefined)
    setOpen(next(project, ask))
  }
  /** True when the ask waits in the draft instead of opening the dialog. */
  const plan = (ask: PlanAsk): boolean => {
    if (!direct && draft?.stage(ask)) return true
    show(ask)
    return false
  }
  const reopen = (ask: PlanAsk) => (mounted.current ? show(ask) : fallback.open(project, ask))
  const dialog = open ? (
    <Suspense fallback={null}>
      <PlanDialog
        key={open.attempt}
        project={project}
        ask={open.ask}
        onClose={() => setOpen(null)}
        onApplied={onApplied}
        reopen={reopen}
        onRefused={setRefusal}
        hooks={hooks}
      />
    </Suspense>
  ) : null
  return { plan, dialog, fields: refusal?.fields ?? [], refusal }
}
