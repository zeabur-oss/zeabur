import { useT, type Translate } from "@/i18n"
import type { Fixer, Origin } from "@/model/origin"

/** Where a cause lies when it is not the reader's own service, as one sentence. */
export function originSentence(t: Translate, origin: Origin): string | undefined {
  return origin === "code" || origin === "configuration" || origin === "unknown"
    ? undefined
    : t(`origin.${origin}.hint`)
}

/** Who must act when it is not the reader, as the words before the fix. */
export function fixLead(t: Translate, fixer: Fixer): string | undefined {
  return fixer === "clusterAdmin" || fixer === "componentOwner"
    ? t(`fixer.${fixer}.lead`)
    : undefined
}

/** Who must act in a row, when it is not the reader; nothing when the reader can fix it. */
export function FixerLine({ fixer }: { fixer: Fixer }) {
  const t = useT()
  return fixer === "clusterAdmin" || fixer === "componentOwner" ? (
    <span className="type-meta text-secondary">{t(`fixer.${fixer}`)}</span>
  ) : null
}
