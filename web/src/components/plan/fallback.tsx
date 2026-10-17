// The shell's plan dialog, for a review a toast asks for after its page has gone (planner.tsx).
// Its own module, so the shell carries this and not the planner.
import { lazy, Suspense, useSyncExternalStore } from "react"

import type { PlanAsk } from "@/components/plan/planner"

const PlanDialog = lazy(() =>
  import("@/components/plan/plan-dialog").then((m) => ({ default: m.PlanDialog })),
)

export interface Open {
  project: string
  ask: PlanAsk
  /** A new plan each time, also for the same ask. */
  attempt: number
}

let attempts = 0
export const next = (project: string, ask: PlanAsk): Open => ({ project, ask, attempt: ++attempts })

export const fallback = (() => {
  let current: Open | null = null
  const listeners = new Set<() => void>()
  const set = (value: Open | null) => {
    current = value
    for (const listener of listeners) listener()
  }
  return {
    open: (project: string, ask: PlanAsk) => set(next(project, ask)),
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
        onClose={fallback.close}
        reopen={(ask) => fallback.open(open.project, ask)}
      />
    </Suspense>
  )
}
