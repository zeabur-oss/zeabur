import CaretRightIcon from "~icons/ph/caret-right"
import CheckCircleFillIcon from "~icons/ph/check-circle-fill"
import QuestionIcon from "~icons/ph/question"
import XCircleFillIcon from "~icons/ph/x-circle-fill"

import { Ago } from "@/components/bits"
import { Tooltip } from "@zeabur/design-system/components/tooltip"
import { useT } from "@/i18n"
import { cn } from "@/lib/utils"
import type { Hop, HopState, Trace } from "@/model/trace"

const stateIcon = {
  ok: CheckCircleFillIcon,
  fail: XCircleFillIcon,
  unknown: QuestionIcon,
} as const

const stateTone: Record<HopState, string> = {
  ok: "text-ok",
  fail: "text-fail",
  unknown: "text-tertiary",
}

/** Each hop from the address to a replica, with what is known of it. */
export function RouteTrace({ trace }: { trace: Trace }) {
  const t = useT()
  const label = (hop: Hop) => {
    switch (hop.kind) {
      case "dns":
        return t("trace.dns")
      case "route":
        return `${hop.object} ${hop.name}`
      case "service":
        return hop.port ? `Service ${hop.name}:${hop.port}` : `Service ${hop.name}`
      case "replicas":
        return t("trace.replicas", hop.ready, hop.desired)
    }
  }
  const note = (hop: Hop) => {
    if (hop.state === "unknown") return t("trace.notChecked")
    if (hop.state === "ok") return undefined
    switch (hop.kind) {
      case "service":
        return t("trace.noService")
      case "replicas":
        return t("trace.noneReady")
      default:
        return t("trace.failed")
    }
  }
  return (
    <div className="flex flex-col gap-1">
      <ol
        className="flex min-w-0 flex-wrap items-center gap-x-1 gap-y-1.5"
        aria-label={t("trace.title")}
      >
        {trace.hops.map((hop, i) => {
          const Icon = stateIcon[hop.state]
          const why = note(hop)
          const chip = (
            <span
              className={cn(
                "flex min-w-0 items-center gap-1.5 rounded-md border border-v2-border px-2 py-0.5 text-xs",
                hop.state === "fail" && "border-fail/40 bg-fail-soft/40",
              )}
            >
              <Icon className={cn("size-3.5 shrink-0", stateTone[hop.state])} aria-hidden="true" />
              <span className="truncate font-mono" translate="no">
                {label(hop)}
              </span>
              {why ? <span className="sr-only">{why}</span> : null}
            </span>
          )
          return (
            <li key={`${hop.kind}-${i}`} className="flex min-w-0 items-center gap-1">
              {i > 0 ? (
                <CaretRightIcon className="size-3 shrink-0 text-tertiary" aria-hidden="true" />
              ) : null}
              {why ? <Tooltip content={why}>{chip}</Tooltip> : chip}
            </li>
          )
        })}
      </ol>
      <p className="type-meta text-tertiary">
        {trace.checkedAt ? (
          <>
            {t("trace.checked")} <Ago iso={trace.checkedAt} />
          </>
        ) : (
          t("trace.fromObjects")
        )}
      </p>
    </div>
  )
}
