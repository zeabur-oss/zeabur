// What a review shows while its code loads (plan-dialog.tsx is a chunk of its own), so Save
// answers at once, and the code's preload for when the page is idle.
import { useEffect } from "react"

import { LoadingRows } from "@/components/page"
import { useT } from "@/i18n"
import { Button } from "@zeabur/design-system/components/button"
import { Dialog, DialogContent, DialogTitle } from "@zeabur/design-system/components/dialog"

export function PlanDialogPending({ title, onClose }: { title: string; onClose: () => void }) {
  const t = useT()
  return (
    <Dialog open onOpenChange={(open) => (open ? undefined : onClose())}>
      <DialogContent variant="compact" aria-describedby={undefined}>
        <div className="border-b px-5 py-3">
          <DialogTitle>{title}</DialogTitle>
        </div>
        <div className="px-5 py-4">
          <LoadingRows label={t("plan.planning")} rows={1} />
        </div>
        <div className="flex items-center justify-end gap-2 border-t px-5 py-3">
          <Button variant="ghost" size="sm" onClick={onClose}>
            {t("plan.cancel")}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
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
