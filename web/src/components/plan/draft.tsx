import { createContext, useContext, useState, type ReactNode } from "react"

import type { Schemas } from "@/api/client"
import type { PlanAsk } from "@/components/plan/planner"
import { useT, type Translate } from "@/i18n"
import { draftable, editCount, mergeChanges, partOf } from "@/model/draft"
import type { DraftPart } from "@/routes/search"

export interface Draft {
  changes: Schemas["Change"][]
  restartAfter: boolean
  count: number
  parts: DraftPart[]
  /** Changes when the draft is applied or discarded, so forms start again from the cluster. */
  generation: number
  /** Whether the ask waited in the draft; otherwise the caller plans it at once. */
  stage: (ask: PlanAsk) => boolean
  discard: () => void
  applied: () => void
}

const DraftContext = createContext<Draft | null>(null)

export const useDraft = () => useContext(DraftContext)

export function useDraftState(): Draft {
  const [changes, setChanges] = useState<Schemas["Change"][]>([])
  const [restartAfter, setRestartAfter] = useState(false)
  const [generation, setGeneration] = useState(0)
  const reset = () => {
    setChanges([])
    setRestartAfter(false)
    setGeneration((g) => g + 1)
  }
  const parts = [...new Set(changes.map(partOf).filter((p): p is DraftPart => p !== undefined))]
  return {
    changes,
    restartAfter,
    count: editCount(changes),
    parts,
    generation,
    stage: (ask) => {
      if (!draftable(ask.changes)) return false
      setChanges((staged) => mergeChanges(staged, ask.changes))
      if (ask.restartAfter) setRestartAfter(true)
      return true
    },
    discard: reset,
    applied: reset,
  }
}

export function DraftProvider({ draft, children }: { draft: Draft; children: ReactNode }) {
  return <DraftContext value={draft}>{children}</DraftContext>
}

/** "Add to changes" where edits wait in a draft, else "Review". */
export function useReviewWord(): string {
  const t = useT()
  return useDraft() ? t("draft.add") : t("edit.review")
}

export function ReviewWord() {
  return useReviewWord()
}

/** The parts the draft changes, in the order of the tabs. */
export const describeDraft = (t: Translate, draft: Pick<Draft, "parts">) =>
  t(
    "common.list",
    partOrder.filter((p) => draft.parts.includes(p)).map((p) => t(`part.${p}`)),
  )

const partOrder: DraftPart[] = ["networking", "variables", "compute", "health-checks", "storage"]
