import { useQuery } from "@tanstack/react-query"
import ShieldCheckFillIcon from "~icons/ph/shield-check-fill"

import { sessionQuery } from "@/api/queries"
import { dropElevation } from "@/auth/elevation"
import { Badge } from "@zeabur/design-system/components/badge"
import { Button } from "@zeabur/design-system/components/button"
import { Popover, PopoverContent, PopoverTrigger } from "@zeabur/design-system/components/popover"
import { useI18n, useT } from "@/i18n"
import { formatClock } from "@/lib/identity"

export function AdminBadge() {
  const t = useT()
  const { locale } = useI18n()
  const session = useQuery(sessionQuery).data
  const until = session?.elevation === "active" ? session.elevatedUntil : undefined
  if (!until) return null
  const time = formatClock(until, locale)
  return (
    <Popover>
      <PopoverTrigger variant="plain">
        <Badge variant="warn" size="lg">
          <ShieldCheckFillIcon aria-hidden="true" />
          {t("elevate.until", time)}
        </Badge>
      </PopoverTrigger>
      <PopoverContent align="end" className="w-64">
        <div className="flex flex-col gap-3 p-2">
          <p className="text-sm">{t("elevate.activeDetail")}</p>
          <Button variant="outline" size="sm" onClick={() => void dropElevation()}>
            {t("elevate.drop")}
          </Button>
        </div>
      </PopoverContent>
    </Popover>
  )
}
