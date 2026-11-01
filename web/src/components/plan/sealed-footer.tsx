import type { ReactNode } from "react"

import { COUNTDOWN_FROM_SECONDS, clock } from "@/components/plan/expiry"
import { useT } from "@/i18n"
import { Button } from "@zeabur-oss/design-system/components/button"

/** The footer of a dialog that applies a sealed plan: the way back, the time left once it runs
 * short, and the confirm button, or Plan again once the plan expired. */
export function SealedFooter({
  left,
  ready,
  back,
  onReplan,
  children,
}: {
  /** Seconds until the plan expires. */
  left: number | undefined
  ready: boolean
  back: ReactNode
  onReplan: () => void
  children: ReactNode
}) {
  const t = useT()
  const expired = left !== undefined && left <= 0
  return (
    <div className="flex w-full flex-wrap items-center gap-2">
      {back}
      {ready && left !== undefined && left < COUNTDOWN_FROM_SECONDS ? (
        <span className="type-meta text-muted-foreground tabular-nums">
          {expired ? t("plan.expired") : t("plan.expiresIn", clock(left))}
        </span>
      ) : null}
      <span className="ml-auto">
        {expired ? (
          <Button size="sm" onClick={onReplan}>
            {t("plan.replan")}
          </Button>
        ) : (
          children
        )}
      </span>
    </div>
  )
}
