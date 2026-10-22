import type { ReactNode } from "react"

import type { PlanStepState } from "@/components/plan/planner"
import { useT } from "@/i18n"
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
