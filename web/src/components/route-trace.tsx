import CaretRightIcon from "~icons/ph/caret-right"
import CheckCircleFillIcon from "~icons/ph/check-circle-fill"
import QuestionIcon from "~icons/ph/question"
import XCircleFillIcon from "~icons/ph/x-circle-fill"

import type { Schemas } from "@/api/client"
import { Button } from "@zeabur/design-system/components/button"
import { Tooltip } from "@zeabur/design-system/components/tooltip"
import { useSentence, useT } from "@/i18n"
import { cn } from "@/lib/utils"

type Hop = Schemas["Hop"]
type HopStatus = Schemas["HopStatus"]

const statusIcon = {
  ok: CheckCircleFillIcon,
  failed: XCircleFillIcon,
  unknown: QuestionIcon,
} as const

const statusTone: Record<HopStatus, string> = {
  ok: "text-ok",
  failed: "text-fail",
  unknown: "text-tertiary",
}

/** Each hop from the address to a ready replica as the API checked it, and where it stops. */
export function RouteTrace({
  route,
  onFix,
}: {
  route: Schemas["RouteTraceView"]
  /** Plans the change the API proposes for the hop that fails. */
  onFix?: () => void
}) {
  const t = useT()
  const write = useSentence()
  const detail = (hop: Hop) =>
    [
      write(hop.message),
      hop.expected ? `${t("trace.expected")}${t("common.colon")}${hop.expected}` : "",
      hop.saw ? `${t("trace.saw")}${t("common.colon")}${hop.saw}` : "",
    ]
      .filter(Boolean)
      .join("\n")
  const stop = route.hops.find((h) => h.status === "failed")
  return (
    <div className="flex flex-col gap-1.5">
      <ol
        className="flex min-w-0 flex-wrap items-center gap-x-1 gap-y-1.5"
        aria-label={t("trace.title")}
      >
        {route.hops.map((hop, i) => {
          const Icon = statusIcon[hop.status]
          return (
            <li key={`${hop.step}-${i}`} className="flex min-w-0 items-center gap-1">
              {i > 0 ? (
                <CaretRightIcon className="size-3 shrink-0 text-tertiary" aria-hidden="true" />
              ) : null}
              <Tooltip content={<span className="whitespace-pre-line">{detail(hop)}</span>}>
                <span
                  className={cn(
                    "flex min-w-0 items-center gap-1.5 rounded-md border border-v2-border px-2 py-0.5 text-xs",
                    hop.status === "failed" && "border-fail/40 bg-fail-soft/40",
                  )}
                >
                  <Icon
                    className={cn("size-3.5 shrink-0", statusTone[hop.status])}
                    aria-hidden="true"
                  />
                  <span className="truncate">{t(`trace.step.${hop.step}`)}</span>
                  <span className="sr-only">{detail(hop)}</span>
                </span>
              </Tooltip>
            </li>
          )
        })}
      </ol>
      {stop ? (
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
          <p className="text-sm text-fail-strong">{write(stop.message)}</p>
          {onFix ? (
            <Button variant="outline" size="sm" onClick={onFix}>
              {t("trace.fix")}
            </Button>
          ) : null}
        </div>
      ) : route.hops.some((h) => h.status === "unknown") ? (
        <p className="type-meta text-tertiary">{t("trace.someUnknown")}</p>
      ) : null}
    </div>
  )
}
