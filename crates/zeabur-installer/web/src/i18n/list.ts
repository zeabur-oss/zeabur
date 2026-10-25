/** A list in Chinese, with a space where its 和 meets a Latin word (「api、web 和 worker」). */
export function listZh(locale: "zh-TW" | "zh-CN", items: string[]): string {
  const parts = new Intl.ListFormat(locale).formatToParts(items)
  return parts
    .map((part, i) => {
      if (part.type !== "literal" || !/\p{Script=Han}/u.test(part.value)) return part.value
      const before = /[!-~]$/.test(parts[i - 1]?.value ?? "") ? " " : ""
      const after = /^[!-~]/.test(parts[i + 1]?.value ?? "") ? " " : ""
      return `${before}${part.value}${after}`
    })
    .join("")
}
