/** A timestamp as the person's locale writes it, with the full RFC 3339 value for a tooltip. */
export function formatTime(iso: string, locale: string): string {
  const date = new Date(iso)
  if (Number.isNaN(date.getTime())) return iso
  return new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeStyle: "short" }).format(date)
}

/** How long ago, in the person's locale ("5 minutes ago"). */
export function formatAgo(iso: string, locale: string, now = Date.now()): string {
  const date = new Date(iso)
  if (Number.isNaN(date.getTime())) return iso
  const seconds = Math.round((date.getTime() - now) / 1000)
  const format = new Intl.RelativeTimeFormat(locale, { numeric: "auto" })
  const units: [Intl.RelativeTimeFormatUnit, number][] = [
    ["day", 86_400],
    ["hour", 3_600],
    ["minute", 60],
  ]
  for (const [unit, size] of units) {
    if (Math.abs(seconds) >= size) return format.format(Math.round(seconds / size), unit)
  }
  return format.format(seconds, "second")
}

/** A day as the person's locale writes it ("Oct 20, 2026"). */
export function formatDate(iso: string, locale: string): string {
  const date = new Date(iso)
  if (Number.isNaN(date.getTime())) return iso
  return new Intl.DateTimeFormat(locale, { dateStyle: "medium" }).format(date)
}
