// A section of the cluster page: a heading with its count and an action at the right, then the
// section's own content, loading, empty or permission state.
import type { ReactNode, Ref } from "react"

import { GateTip } from "@/components/gate"
import { useT } from "@/i18n"
import type { Gate } from "@/model/rights"
import { Button } from "@zeabur-oss/design-system/components/button"

export function Section({
  id,
  title,
  count,
  action,
  children,
  ref,
}: {
  id: string
  title: ReactNode
  count?: ReactNode
  action?: ReactNode
  children: ReactNode
  ref?: Ref<HTMLElement>
}) {
  return (
    <section ref={ref} id={id} aria-labelledby={`${id}-title`} className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-x-2.5 gap-y-2">
        <h2 id={`${id}-title`} className="type-heading">
          {title}
        </h2>
        {count !== undefined ? (
          <span className="type-meta text-muted-foreground">{count}</span>
        ) : null}
        {action ? <div className="ml-auto flex items-center">{action}</div> : null}
      </div>
      {children}
    </section>
  )
}

/** A section form's buttons: cancel, when it can be closed, and review. */
export function FormButtons({ gate, onCancel }: { gate: Gate; onCancel?: () => void }) {
  const t = useT()
  return (
    <div className="flex justify-end gap-2">
      {onCancel ? (
        <Button size="sm" variant="ghost" type="button" onClick={onCancel}>
          {t("clusterBackups.cancel")}
        </Button>
      ) : null}
      <GateTip gate={gate}>
        <Button size="sm" type="submit">
          {t("edit.review")}
        </Button>
      </GateTip>
    </div>
  )
}
