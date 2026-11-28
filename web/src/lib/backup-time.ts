// When a backup runs. Controllers read cron in UTC; the form shows the browser's time.
import type { Translate } from "@/i18n"

const DAY = 24 * 60
const WEEK = 7 * DAY

/** Daily, or weekly on `day` (Sunday 0), at `minute` minutes past midnight, all in UTC. */
export interface BackupTime {
  weekly: boolean
  day: number
  minute: number
}

/** 03:00 UTC daily; weekly on Sunday. */
export const DEFAULT_TIME: BackupTime = { weekly: false, day: 0, minute: 3 * 60 }

const mod = (n: number, m: number) => ((n % m) + m) % m

/** The time as cron, with a seconds field first for CloudNativePG. */
export function backupCron(time: BackupTime, seconds = false): string {
  const fields = [time.minute % 60, Math.floor(time.minute / 60), "*", "*"]
  return [...(seconds ? [0] : []), ...fields, time.weekly ? time.day : "*"].join(" ")
}

const field = (text: string | undefined, max: number) =>
  text !== undefined && /^\d{1,2}$/.test(text) && Number(text) <= max ? Number(text) : undefined

/** The time `cron` runs, when it is one minute of every day or of one weekday. */
export function backupTimeOf(cron: string): BackupTime | undefined {
  let fields = cron.trim().split(/\s+/)
  if (fields.length === 6 && fields[0] === "0") fields = fields.slice(1)
  if (fields.length !== 5 || fields[2] !== "*" || fields[3] !== "*") return undefined
  const minute = field(fields[0], 59)
  const hour = field(fields[1], 23)
  if (minute === undefined || hour === undefined) return undefined
  const weekly = fields[4] !== "*"
  const day = weekly ? field(fields[4], 7) : 0
  if (day === undefined) return undefined
  return { weekly, day: day % 7, minute: hour * 60 + minute }
}

/** `time` moved by `minutes`, across midnight onto the next or previous day. */
export function shiftTime(time: BackupTime, minutes: number): BackupTime {
  const at = mod(time.day * DAY + time.minute + minutes, WEEK)
  return { weekly: time.weekly, day: Math.floor(at / DAY), minute: at % DAY }
}

/** The browser's offset from UTC now, in minutes east. */
export const browserOffset = (now = new Date()) => -now.getTimezoneOffset()

/** "03:00" for `minute` minutes past midnight. */
export const clock = (minute: number) =>
  `${String(Math.floor(minute / 60)).padStart(2, "0")}:${String(minute % 60).padStart(2, "0")}`

/** The minutes past midnight of an `<input type="time">` value. */
export function clockMinute(value: string): number | undefined {
  const match = /^(\d{2}):(\d{2})/.exec(value)
  if (!match) return undefined
  const minute = Number(match[1]) * 60 + Number(match[2])
  return minute < DAY ? minute : undefined
}

/** "Sun 17:00 UTC", or "17:00 UTC" daily. */
export const utcWords = (t: Translate, time: BackupTime) =>
  t("backupTime.utc", time.weekly ? time.day : undefined, clock(time.minute))

/** "Every Monday at 01:00 (Sun 17:00 UTC)": in the browser's time, then in UTC. */
export function timeWords(t: Translate, time: BackupTime, offset = browserOffset()): string {
  const local = shiftTime(time, offset)
  const words = local.weekly
    ? t("backupTime.weekly", local.day, clock(local.minute))
    : t("backupTime.daily", clock(local.minute))
  return offset === 0
    ? t("backupTime.inUtc", words)
    : t("backupTime.both", words, utcWords(t, time))
}
