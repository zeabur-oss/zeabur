// What the three catalogues' hand-written problem sentences share: the parameters' type and the
// helpers that read them. Diagnoses, gaps, traces and incidents are in crates/zeabur-gaps/messages.

/** A server sentence's parameters, by name. */
export type P = Record<string, string>

/** The object a problem names in Chinese: its name, else its resource (in its namespace). */
export const thingZh = (p: P, here: string) =>
  p.name ?? (p.resource ? `${p.namespace ? `${p.namespace} ${here} ` : ""}${p.resource}` : "")

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
