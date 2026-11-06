// What a review shows while its code loads (plan-dialog.tsx is a chunk of its own), so Save
// answers at once, and the code's preload for when the page is idle.
import XIcon from "~icons/ph/x"
import { useEffect } from "react"

import { LoadingRows } from "@/components/page"
import { useT } from "@/i18n"
import { Button } from "@zeabur-oss/design-system/components/button"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogTitle,
} from "@zeabur-oss/design-system/components/dialog"

/** The same frame as PlanDialog, with its action disabled, so nothing moves under the pointer. */
export function PlanDialogPending({ title, onClose }: { title: string; onClose: () => void }) {
  const t = useT()
  return (
    <Dialog open onOpenChange={(open) => (open ? undefined : onClose())}>
      <DialogContent variant="compact" aria-describedby={undefined}>
        <div className="flex items-center gap-2 border-b px-5 py-3">
          <div className="min-w-0 flex-1 truncate">
            <DialogTitle>{title}</DialogTitle>
          </div>
          <DialogClose
            render={<Button variant="ghost" size="icon-sm" aria-label={t("common.close")} />}
          >
            <XIcon aria-hidden="true" />
          </DialogClose>
        </div>
        <div className="px-5 py-4">
          <LoadingRows label={t("plan.planning")} rows={1} />
        </div>
        <div className="flex items-center justify-end gap-2 border-t px-5 py-3">
          <Button variant="ghost" size="sm" onClick={onClose}>
            {t("plan.cancel")}
          </Button>
          <Button size="sm" disabled>
            {t("plan.apply")}
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
