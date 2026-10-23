import type { ReactNode } from "react"

import type { PlanStepState } from "@/components/plan/planner"
import { useT } from "@/i18n"
import { Button } from "@zeabur/design-system/components/button"
import { Drawer } from "@zeabur/design-system/components/drawer"

/** A form in a drawer whose plan is the drawer's second step, with Back to the form. */
export function PlanDrawer({
  step,
  title,
  description,
  footer,
  onClose,
  children,
}: {
  step: PlanStepState
  title: ReactNode
  description?: ReactNode
  footer: ReactNode
  onClose: () => void
  children: ReactNode
}) {
  const t = useT()
  const asked = step.asked
  return (
    <Drawer
      open
      onOpenChange={(next) => {
        if (!next) onClose()
      }}
      title={asked ? asked.title : title}
      description={asked ? undefined : description}
      closeLabel={t("common.close")}
      footer={asked ? <div ref={step.footerRef} className="w-full" /> : footer}
    >
      <div hidden={!!asked}>{children}</div>
      {step.view}
    </Drawer>
  )
}

/** A plan drawer's form: Enter reviews the changes, as the footer's button does. */
export function ReviewForm({ onReview, children }: { onReview: () => void; children: ReactNode }) {
  return (
    <form
      className="flex flex-col gap-4"
      onSubmit={(event) => {
        event.preventDefault()
        onReview()
      }}
    >
      {children}
    </form>
  )
}

/** A plan drawer's footer: cancel, or review the changes. `note` goes on the left. */
export function ReviewFooter({
  onCancel,
  onReview,
  disabled,
  note,
}: {
  onCancel: () => void
  onReview: () => void
  disabled?: boolean
  note?: ReactNode
}) {
  const t = useT()
  return (
    <div className="flex items-center justify-end gap-2">
      {note ? <span className="mr-auto type-meta text-muted-foreground">{note}</span> : null}
      <Button variant="ghost" size="sm" onClick={onCancel}>
        {t("edit.cancel")}
      </Button>
      <Button size="sm" disabled={disabled} onClick={onReview}>
        {t("edit.review")}
      </Button>
    </div>
  )
}
