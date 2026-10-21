import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from "react"

import type { Schemas } from "@/api/client"
import type { PlanAsk } from "@/components/plan/planner"
import { useT, type Translate } from "@/i18n"
import { draftable, dropChanges, editCount, mergeChanges, partOf, partOrder } from "@/model/draft"
import type { DraftPart } from "@/routes/search"

/** Which staged changes a form's edits stand in for: these ops, of this workload (and probe). */
interface Scope {
  ops: Schemas["Change"]["op"][]
  workload: Schemas["WorkloadRef"]
  probe?: Schemas["ProbeKind"]
}

/** A form's edits as typed, which stand in for the staged changes in its scope. */
export interface Held {
  changes: Schemas["Change"][]
  scope: Scope
  /** Why they cannot be saved yet. */
  blocked?: string
  /** An editor is open, so the bar shows even with nothing changed yet. */
  editing?: boolean
}

export function covers(scope: Scope, change: Schemas["Change"]): boolean {
  if (!scope.ops.includes(change.op)) return false
  const workload = "workload" in change ? change.workload : undefined
  if (workload && (workload.kind !== scope.workload.kind || workload.name !== scope.workload.name))
    return false
  return scope.probe === undefined || (change.op === "probe" && change.probe === scope.probe)
}

export interface Draft {
  changes: Schemas["Change"][]
  restartAfter: boolean
  count: number
  parts: DraftPart[]
  /** Changes when the draft is applied or discarded, so forms start again from the cluster. */
  generation: number
  /** Whether the ask waited in the draft; otherwise the caller plans it at once. */
  stage: (ask: PlanAsk) => boolean
  /** Takes staged changes back out. */
  unstage: (changes: Schemas["Change"][]) => void
  discard: () => void
  applied: () => void
  /** Whether an editor is open. */
  editing: boolean
  /** Why the held edits cannot be saved yet, the first form's reason. */
  blocked?: string
  /** Sets or clears one form's held edits; stable across renders. */
  hold: (key: string, held: Held | undefined) => void
  /** The staged changes with the held ones in place of those they cover. */
  withHeld: () => Schemas["Change"][]
}

const DraftContext = createContext<Draft | null>(null)

export const useDraft = () => useContext(DraftContext)

export function useDraftState(): Draft {
  const [changes, setChanges] = useState<Schemas["Change"][]>([])
  const [restartAfter, setRestartAfter] = useState(false)
  const [generation, setGeneration] = useState(0)
  const [holds, setHolds] = useState<Record<string, Held>>({})
  const hold = useCallback(
    (key: string, held: Held | undefined) =>
      setHolds((previous) => {
        if (!held && !(key in previous)) return previous
        const next = { ...previous }
        if (held) next[key] = held
        else delete next[key]
        return next
      }),
    [],
  )
  const held = Object.values(holds)
  const withHeld = () => [
    ...changes.filter((c) => !held.some((h) => covers(h.scope, c))),
    ...held.flatMap((h) => h.changes),
  ]
  const reset = () => {
    setHolds({})
    setChanges([])
    setRestartAfter(false)
    setGeneration((g) => g + 1)
  }
  const all = withHeld()
  const parts = [...new Set(all.map(partOf).filter((p): p is DraftPart => p !== undefined))]
  return {
    changes,
    restartAfter,
    count: editCount(all),
    parts,
    generation,
    stage: (ask) => {
      if (!draftable(ask.changes)) return false
      setChanges((staged) => mergeChanges(staged, ask.changes))
      if (ask.restartAfter) setRestartAfter(true)
      return true
    },
    unstage: (dropped) => setChanges((staged) => dropChanges(staged, dropped)),
    discard: reset,
    applied: reset,
    editing: held.some((h) => h.editing),
    blocked: held.find((h) => h.blocked)?.blocked,
    hold,
    withHeld,
  }
}

/** Holds a form's edits in the draft while it is mounted; `key` names the form. */
export function useHold(key: string, held: Held | undefined) {
  const hold = useDraft()?.hold
  const value = JSON.stringify(held ?? null)
  useEffect(() => {
    hold?.(key, (JSON.parse(value) as Held | null) ?? undefined)
  }, [hold, key, value])
  useEffect(() => () => hold?.(key, undefined), [hold, key])
}

export function DraftProvider({ draft, children }: { draft: Draft; children: ReactNode }) {
  return <DraftContext value={draft}>{children}</DraftContext>
}

/** "Add to changes" where edits wait in a draft, else "Review". */
export function ReviewWord() {
  const t = useT()
  return useDraft() ? t("draft.add") : t("edit.review")
}

/** The parts the draft changes, in the order of the tabs. */
export const describeDraft = (t: Translate, draft: Pick<Draft, "parts">) =>
  t(
    "common.list",
    partOrder.filter((p) => draft.parts.includes(p)).map((p) => t(`part.${p}`)),
  )
