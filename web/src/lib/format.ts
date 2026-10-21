import type { Translate } from "@/i18n"

export function formatTime(iso: string, locale: string): string {
  const date = new Date(iso)
  if (Number.isNaN(date.getTime())) return iso
  return new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeStyle: "short" }).format(date)
}

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

export function formatDate(iso: string, locale: string): string {
  const date = new Date(iso)
  if (Number.isNaN(date.getTime())) return iso
  return new Intl.DateTimeFormat(locale, { dateStyle: "medium" }).format(date)
}

/** "Updated just now", "Updated 3m ago": how long ago the page last heard from the cluster. */
export function updatedWords(t: Translate, updatedAt: number, now: number): string {
  const seconds = Math.max(0, Math.floor((now - updatedAt) / 1000))
  if (seconds < 60) return t("freshness.justNow")
  if (seconds < 3_600) return t("freshness.ago", Math.floor(seconds / 60), "minute")
  if (seconds < 86_400) return t("freshness.ago", Math.floor(seconds / 3_600), "hour")
  return t("freshness.ago", Math.floor(seconds / 86_400), "day")
}
