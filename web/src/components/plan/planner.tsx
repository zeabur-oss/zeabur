// A toast's "Review again" uses the page's dialog while the page is mounted, else the shell's
// (`PlanFallback` in fallback.tsx).
import { lazy, Suspense, useEffect, useRef, useState } from "react"

import type { Schemas } from "@/api/client"
import { fallback, next, type Open } from "@/components/plan/fallback"

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
}

export function usePlanner(project: string, onApplied?: () => void) {
  const [open, setOpen] = useState<Open | null>(null)
  const [fields, setFields] = useState<Schemas["FieldCause"][]>([])
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
