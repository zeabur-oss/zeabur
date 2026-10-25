// What the three catalogues' hand-written problem sentences share: the parameters' type and the
// helpers that read them. Diagnoses, gaps, traces and incidents are in src/i18n/server.

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

/** The items of a comma-separated parameter. */
export const items = (value: string | undefined) => (value ?? "").split(",")

/** `words` joined by `separator`, the last one by `last`. */
export const joinWords = (words: string[], separator: string, last: string) =>
  words.length > 1
    ? `${words.slice(0, -1).join(separator)}${last}${words.at(-1)}`
    : words.join(separator)

/** A number of seconds as whole minutes when it is, else as seconds. */
export function durationOf(
  value: string | undefined,
  seconds: (n: number) => string,
  minutes: (n: number) => string,
) {
  const n = Number(value)
  return n >= 60 && n % 60 === 0 ? minutes(n / 60) : seconds(n)
}
