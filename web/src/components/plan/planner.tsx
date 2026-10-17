// Every write goes the same way: a page describes the change, Engawa plans it (writes nothing),
// the person reads the plan and confirms it, and it is applied by its token. `usePlanner` gives a
// page the one call it needs and the dialog to render; the dialog's code loads when the first
// plan opens. The dialog closes on confirm and the outcome is a toast (outcome.ts), whose
// "Review again" plans the same change afresh: in the page's own dialog while the page is there,
// else in the shell's (`PlanFallback` in fallback.tsx), so it still opens after the page or sheet
// has gone.
import { lazy, Suspense, useEffect, useRef, useState } from "react"

import type { Schemas } from "@/api/client"
import { fallback, next, type Open } from "@/components/plan/fallback"

const PlanDialog = lazy(() =>
  import("@/components/plan/plan-dialog").then((m) => ({ default: m.PlanDialog })),
)

/** What a page asks to change, and how the dialog names it ("Restart talker"). */
export interface PlanAsk {
  title: string
  changes: Schemas["Change"][]
  /** Also restart the services that read a changed ConfigMap or Secret into variables. */
  restartAfter?: boolean
  /** The service the change is asked from, when the changes do not name it (a ConfigMap, a
   * route, a policy): "also affects" leaves it out, and the toast's View goes to its page. */
  service?: Schemas["WorkloadRef"]
}

export function usePlanner(project: string, onApplied?: () => void) {
  const [open, setOpen] = useState<Open | null>(null)
  // The fields the cluster refused in the last plan, for the form to mark (`refusedField`).
  const [fields, setFields] = useState<Schemas["FieldCause"][]>([])
  // Whether the page is still there when a toast asks to review again.
  const mounted = useRef(false)
  useEffect(() => {
    mounted.current = true
    return () => {
      mounted.current = false
    }
  }, [])
  const plan = (ask: PlanAsk) => {
    setFields([])
    setOpen(next(project, ask))
  }
  const reopen = (ask: PlanAsk) => (mounted.current ? plan(ask) : fallback.open(project, ask))
  const dialog = open ? (
    <Suspense fallback={null}>
      <PlanDialog
        key={open.attempt}
        project={project}
        ask={open.ask}
        onClose={() => setOpen(null)}
        onApplied={onApplied}
        reopen={reopen}
        onRefused={setFields}
      />
    </Suspense>
  ) : null
  return { plan, dialog, fields }
}
