import CaretRightIcon from "~icons/ph/caret-right"
import WarningCircleIcon from "~icons/ph/warning-circle"

import type { Schemas } from "@/api/client"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur-oss/design-system/components/collapsible"
import { useT } from "@/i18n"

/** Objects of another product that this version of Zeabur cannot read, with why. */
export function Undecoded({ undecoded }: { undecoded: Schemas.UndecodedView[] }) {
  const t = useT()
  if (undecoded.length === 0) return null
  return (
    <div data-undecoded="" className="text-sm">
      <Collapsible>
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5">
          <p className="flex min-w-0 items-start gap-2 text-muted-foreground">
            <WarningCircleIcon aria-hidden="true" className="mt-0.5 size-4 shrink-0" />
            <span>{t("undecoded.line")}</span>
          </p>
          <CollapsibleTrigger variant="disclosure">
            <CaretRightIcon aria-hidden="true" />
            {t("refused.details")}
          </CollapsibleTrigger>
        </div>
        <CollapsibleContent>
          <ul className="flex flex-col gap-1 pt-3 font-mono text-xs text-muted-foreground">
            {undecoded.map((u) => (
              <li key={`${u.origin.kind}/${u.origin.name}`} className="break-words" lang="en">
                {u.origin.kind} {u.origin.name}: {u.error}
              </li>
            ))}
          </ul>
        </CollapsibleContent>
      </Collapsible>
    </div>
  )
}
