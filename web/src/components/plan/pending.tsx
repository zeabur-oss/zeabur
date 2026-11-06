// What a review shows while its code loads (plan-dialog.tsx is a chunk of its own), so Save
// answers at once, and the code's preload for when the page is idle.
import { useEffect } from "react"

import { Frame } from "@/components/members/frame"
import { LoadingRows } from "@/components/page"
import { useT } from "@/i18n"
import { Button } from "@zeabur-oss/design-system/components/button"

/** The same frame as PlanDialog, with its action disabled, so nothing moves under the pointer. */
export function PlanDialogPending({ title, onClose }: { title: string; onClose: () => void }) {
  const t = useT()
  return (
    <Frame
      title={title}
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" size="sm" onClick={onClose}>
            {t("plan.cancel")}
          </Button>
          <Button size="sm" disabled>
            {t("plan.apply")}
          </Button>
        </>
      }
    >
      <LoadingRows label={t("plan.planning")} rows={1} />
    </Frame>
  )
}

export function PlanStepPending() {
  const t = useT()
  return <LoadingRows label={t("plan.planning")} rows={3} />
}

/** Fetches the review's code once the browser has nothing else to do. */
export function usePreloadPlan() {
  useEffect(() => {
    const load = () => void import("@/components/plan/plan-dialog")
    if ("requestIdleCallback" in window) {
      const id = window.requestIdleCallback(load)
      return () => window.cancelIdleCallback(id)
    }
    const id = setTimeout(load, 1000)
    return () => clearTimeout(id)
  }, [])
}
