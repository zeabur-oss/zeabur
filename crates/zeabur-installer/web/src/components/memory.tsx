import type { Health } from "@/components/bits"
import { StatusDot } from "@/components/bits"
import { useT, type Translate } from "@/i18n"
import type { Fit, Verdict } from "@/lib/memory"

export const fitHealth: Record<Verdict, Health> = { fits: "ok", tight: "warn", over: "fail" }

/** "Does not fit, about 2,050 of 2,048 MiB" with its dot. */
export function MemoryVerdict({ fit }: { fit: Fit }) {
  const t = useT()
  return (
    <span className="inline-flex items-center gap-2">
      <StatusDot health={fitHealth[fit.verdict]} />
      <span>
        {t(`memory.${fit.verdict}`)}
        {t("common.sep")}
        {t("memory.usage", fit.used, fit.host)}
      </span>
    </span>
  )
}

/** Why it is tight or over, and what would make it fit; nothing when it fits. */
export function memoryAdvice(t: Translate, fit: Fit): string {
  if (fit.verdict === "fits") return ""
  const cause = fit.verdict === "over" ? t("memory.overBy", fit.short) : t("memory.tightBy")
  return t(
    "common.sentences",
    fit.relief
      ? [
          cause,
          t("memory.relief", t("common.list", fit.relief.names), fit.relief.mib),
          t("memory.more"),
        ]
      : [cause, t("memory.add")],
  )
}
