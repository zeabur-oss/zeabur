// Cron expressions in words, for the common shapes only; anything else stays as written.
// Five fields, or six with seconds first as CloudNativePG writes them (robfig/cron).
import type { Translate } from "@/i18n"

export type CronPlan =
  | { every: "minute" }
  | { every: "minutes"; n: number }
  | { every: "hour"; minute: number }
  | { every: "hours"; n: number; minute: number }
  | { every: "day"; times: string[] }
  | { every: "week"; days: number[]; range: boolean; times: string[] }
  | { every: "month"; days: number[]; times: string[] }

export interface Cron {
  plan: CronPlan
  /** `CRON_TZ=` or `TZ=` in front of the fields; else the controller's clock, taken as UTC. */
  zone: string
}

const macros: Record<string, string> = {
  "@monthly": "0 0 1 * *",
  "@weekly": "0 0 * * 0",
  "@daily": "0 0 * * *",
  "@midnight": "0 0 * * *",
  "@hourly": "0 * * * *",
}

const dayNames = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"]

const any = (field: string) => field === "*" || field === "?"

function number(text: string, min: number, max: number): number | undefined {
  if (!/^\d+$/.test(text)) return undefined
  const n = Number(text)
  return n >= min && n <= max ? n : undefined
}

function numbers(field: string, min: number, max: number): number[] | undefined {
  const out: number[] = []
  for (const part of field.split(",")) {
    const n = number(part, min, max)
    if (n === undefined) return undefined
    out.push(n)
  }
  return [...new Set(out)].toSorted((a, b) => a - b)
}

function step(field: string, divides: number): number | undefined {
  const match = /^\*\/(\d+)$/.exec(field)
  const n = match ? Number(match[1]) : undefined
  return n && n > 1 && divides % n === 0 ? n : undefined
}

function weekday(text: string): number | undefined {
  const named = dayNames.indexOf(text.toLowerCase())
  if (named >= 0) return named
  const n = number(text, 0, 7)
  return n === 7 ? 0 : n
}

function weekdays(field: string): { days: number[]; range: boolean } | undefined {
  const range = /^(\w+)-(\w+)$/.exec(field)
  if (range) {
    const from = weekday(range[1] ?? "")
    const to = weekday(range[2] ?? "")
    if (from === undefined || to === undefined || to <= from) return undefined
    return { days: Array.from({ length: to - from + 1 }, (_, i) => from + i), range: true }
  }
  const days: number[] = []
  for (const part of field.split(",")) {
    const day = weekday(part)
    if (day === undefined) return undefined
    days.push(day)
  }
  return { days: [...new Set(days)].toSorted((a, b) => a - b), range: false }
}

const pad = (n: number) => String(n).padStart(2, "0")

export function parseCron(expression: string): Cron | undefined {
  let text = expression.trim()
  let zone = "UTC"
  const tz = /^(?:CRON_TZ|TZ)=(\S+)\s+(.*)$/.exec(text)
  if (tz) {
    zone = tz[1] ?? zone
    text = tz[2] ?? ""
  }
  text = macros[text] ?? text
  let fields = text.split(/\s+/)
  if (fields.length === 6) {
    if (fields[0] !== "0") return undefined
    fields = fields.slice(1)
  }
  if (fields.length !== 5) return undefined
  const [minute = "", hour = "", dom = "", month = "", dow = ""] = fields
  if (!any(month)) return undefined

  const plan = ((): CronPlan | undefined => {
    if (!any(dom) && !any(dow)) return undefined
    if (any(dom) && any(dow)) {
      if (minute === "*" && hour === "*") return { every: "minute" }
      const minutes = step(minute, 60)
      if (minutes && hour === "*") return { every: "minutes", n: minutes }
      const m = number(minute, 0, 59)
      if (m !== undefined && hour === "*") return { every: "hour", minute: m }
      const hours = step(hour, 24)
      if (m !== undefined && hours) return { every: "hours", n: hours, minute: m }
    }
    const ms = numbers(minute, 0, 59)
    const hs = numbers(hour, 0, 23)
    if (!ms || !hs || ms.length * hs.length > 4) return undefined
    const times = hs.flatMap((h) => ms.map((m) => `${pad(h)}:${pad(m)}`))
    if (!any(dow)) {
      const days = weekdays(dow)
      return days ? { every: "week", ...days, times } : undefined
    }
    if (!any(dom)) {
      const days = numbers(dom, 1, 31)
      return days ? { every: "month", days, times } : undefined
    }
    return { every: "day", times }
  })()
  return plan ? { plan, zone } : undefined
}

/** "Every day at 03:00 (UTC)", or undefined when the expression is not one of the common shapes. */
export function describeCron(t: Translate, expression: string): string | undefined {
  const cron = parseCron(expression)
  if (!cron) return undefined
  const { plan } = cron
  const words = (() => {
    switch (plan.every) {
      case "minute":
        return t("cron.minute")
      case "minutes":
        return t("cron.minutes", plan.n)
      case "hour":
        return t("cron.hour", plan.minute)
      case "hours":
        return t("cron.hours", plan.n, plan.minute)
      case "day":
        return t("cron.day", plan.times)
      case "week":
        return plan.range
          ? t("cron.weekRange", plan.days[0] ?? 0, plan.days.at(-1) ?? 0, plan.times)
          : t("cron.week", plan.days, plan.times)
      case "month":
        return t("cron.month", plan.days, plan.times)
    }
  })()
  return t("cron.zone", words, cron.zone)
}
