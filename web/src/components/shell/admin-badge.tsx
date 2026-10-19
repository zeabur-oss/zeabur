// "Admin until 14:05" in the top bar while requests use admin rights, with a way to drop them
// early. Nothing shows otherwise, and nothing at all on this machine (`engawactl ui`).
import { useQuery } from "@tanstack/react-query"
import ShieldCheckFillIcon from "~icons/ph/shield-check-fill"

import { sessionQuery } from "@/api/queries"
import { dropElevation } from "@/auth/elevation"
import { Badge } from "@engawahq/design-system/components/badge"
import { Button } from "@engawahq/design-system/components/button"
import { Popover, PopoverContent, PopoverTrigger } from "@engawahq/design-system/components/popover"
import { useI18n, useT } from "@/i18n"

export function AdminBadge() {
  const t = useT()
  const { locale } = useI18n()
  const session = useQuery(sessionQuery).data
  const until = session?.elevation === "active" ? session.elevatedUntil : undefined
  if (!until) return null
  const time = new Intl.DateTimeFormat(locale, { hour: "2-digit", minute: "2-digit" }).format(
    new Date(until),
  )
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
