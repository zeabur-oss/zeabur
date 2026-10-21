// What the three catalogues' server sentences share: the parameters' type and the helpers that
// read them. The words are in each catalogue.

/** A server sentence's parameters, by name. */
export type P = Record<string, string>

/** The `cuts` of a `gap.noNetworkPolicy` (comma-separated words), each in `words`, joined. */
export const cuts = (p: P, words: Record<string, string>, join: (items: string[]) => string) =>
  join(
    (p.cuts ?? "")
      .split(",")
      .map((cut) => words[cut])
      .filter((word): word is string => !!word),
  )

/** The object a problem names in Chinese: its name, else its resource (in its namespace). */
export const thingZh = (p: P, here: string) =>
  p.name ?? (p.resource ? `${p.namespace ? `${p.namespace} ${here} ` : ""}${p.resource}` : "")

/**
 * A probe's timing as `diagnosis.probeFailure.why.*` sends it: its period in seconds and failure
 * threshold, each with whether it is Kubernetes' default; nothing when the server could not read
 * the probe.
 */
export interface ProbeTiming {
  period: number
  threshold: number
  total: number
  /** Both values are the defaults. */
  defaults: boolean
  /** Only the period, or only the threshold, is the default. */
  periodOnly: boolean
  thresholdOnly: boolean
}
export const probeTiming = (p: P): ProbeTiming | undefined => {
  const period = Number(p.period)
  const threshold = Number(p.threshold)
  if (!p.period || !p.threshold || !period || !threshold) return undefined
  const periodDefault = p.periodDefault === "true"
  const thresholdDefault = p.thresholdDefault === "true"
  return {
    period,
    threshold,
    total: period * threshold,
    defaults: periodDefault && thresholdDefault,
    periodOnly: periodDefault && !thresholdDefault,
    thresholdOnly: thresholdDefault && !periodDefault,
  }
}

/** A list in Chinese, with a space where its 和 meets a Latin word (「api、web 和 worker」). */
export function listZh(locale: "zh-TW" | "zh-CN", items: string[]): string {
  const parts = new Intl.ListFormat(locale).formatToParts(items)
  return parts
    .map((part, i) => {
      if (part.type !== "literal" || !/\p{Script=Han}/u.test(part.value)) return part.value
      const before = /[A-Za-z0-9]$/.test(parts[i - 1]?.value ?? "") ? " " : ""
      const after = /^[A-Za-z0-9]/.test(parts[i + 1]?.value ?? "") ? " " : ""
      return `${before}${part.value}${after}`
    })
    .join("")
}
