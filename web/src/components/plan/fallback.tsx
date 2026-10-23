// The shell's plan dialog, for a review a toast asks for after its page has gone (planner.tsx).
// Its own module, so the shell carries this and not the planner.
import { lazy, Suspense, useSyncExternalStore } from "react"

import type { PlanAsk } from "@/components/plan/planner"
import type { Reviewed } from "@/components/plan/replan"

const PlanDialog = lazy(() =>
  import("@/components/plan/plan-dialog").then((m) => ({ default: m.PlanDialog })),
)

export interface Open {
  project: string
  ask: PlanAsk
  /** A new plan each time, also for the same ask. */
  attempt: number
  /** The plan an apply refused because the project moved on since. */
  reviewed?: Reviewed
}

let attempts = 0
export const next = (project: string, ask: PlanAsk, reviewed?: Reviewed): Open => ({
  project,
  ask,
  attempt: ++attempts,
  reviewed,
})

export const fallback = (() => {
  let current: Open | null = null
  const listeners = new Set<() => void>()
  const set = (value: Open | null) => {
    current = value
    for (const listener of listeners) listener()
  }
  return {
    open: (project: string, ask: PlanAsk, reviewed?: Reviewed) => set(next(project, ask, reviewed)),
    close: () => set(null),
    get: () => current,
    subscribe: (listener: () => void) => {
      listeners.add(listener)
      return () => {
        listeners.delete(listener)
      }
    },
  }
})()

/** Mounted once in the shell: the plan dialog a toast reopens when its page is gone. */
export function PlanFallback() {
  const open = useSyncExternalStore(fallback.subscribe, fallback.get)
  if (!open) return null
  return (
    <Suspense fallback={null}>
      <PlanDialog
        key={open.attempt}
        project={open.project}
        ask={open.ask}
        reviewed={open.reviewed}
        onClose={fallback.close}
        reopen={(ask, reviewed) => fallback.open(open.project, ask, reviewed)}
      />
    </Suspense>
  )
}
