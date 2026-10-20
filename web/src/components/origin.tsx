import { Badge } from "@zeabur/design-system/components/badge"
import { Tooltip } from "@zeabur/design-system/components/tooltip"
import { useT } from "@/i18n"
import type { Fixer, Origin } from "@/model/origin"

/** Where a problem comes from and who can fix it, as two quiet tags. */
export function OriginChips({ origin, fixer }: { origin: Origin; fixer: Fixer }) {
  const t = useT()
  return (
    <span className="flex flex-wrap items-center gap-1">
      <Tooltip content={t(`origin.${origin}.hint`)}>
        <Badge variant="outline">{t(`origin.${origin}`)}</Badge>
      </Tooltip>
      {fixer === "unknown" ? null : (
        <Badge variant={fixer === "you" ? "secondary" : "warn"}>{t(`fixer.${fixer}`)}</Badge>
      )}
    </span>
  )
}
