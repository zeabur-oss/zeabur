import { describe, expect, test } from "bun:test"

import { translator } from "@/i18n"
import { en } from "@/i18n/en"
import { zhTW } from "@/i18n/zh-TW"
import {
  backupCron,
  backupTimeOf,
  clockMinute,
  DEFAULT_TIME,
  shiftTime,
  timeWords,
} from "@/lib/backup-time"

const t = translator(en)
const taipei = 8 * 60
const newYork = -5 * 60

describe("backup times", () => {
  test("any minute of a day or a week is cron in UTC and reads back", () => {
    const weekly = { weekly: true, day: 6, minute: 17 * 60 + 45 }
    expect(backupCron(weekly)).toBe("45 17 * * 6")
    expect(backupCron(weekly, true)).toBe("0 45 17 * * 6")
    expect(backupTimeOf("45 17 * * 6")).toEqual(weekly)
    expect(backupTimeOf("0 45 17 * * 6")).toEqual(weekly)
    expect(backupTimeOf("0 3 * * 7")).toEqual({ weekly: true, day: 0, minute: 180 })
    expect(backupCron(DEFAULT_TIME)).toBe("0 3 * * *")
  })

  test("other shapes are no backup time", () => {
    for (const cron of ["0 3 1 * *", "0 3 * * 1-5", "*/5 * * * *", "0 3,15 * * *", "0 24 * * *"])
      expect(backupTimeOf(cron)).toBeUndefined()
  })

  test("a time in the browser's zone moves the UTC day across midnight", () => {
    // Monday 01:00 in Taipei is Sunday 17:00 UTC.
    const monday = { weekly: true, day: 1, minute: 60 }
    const utc = shiftTime(monday, -taipei)
    expect(utc).toEqual({ weekly: true, day: 0, minute: 17 * 60 })
    expect(shiftTime(utc, taipei)).toEqual(monday)
    // Sunday 22:00 in New York is Monday 03:00 UTC; Saturday wraps to Sunday.
    expect(shiftTime({ weekly: true, day: 0, minute: 22 * 60 }, -newYork).day).toBe(1)
    expect(shiftTime({ weekly: true, day: 6, minute: 23 * 60 }, 120).day).toBe(0)
  })

  test("words give the browser's time, then UTC", () => {
    const sunday = { weekly: true, day: 0, minute: 17 * 60 }
    expect(timeWords(t, sunday, taipei)).toBe("Every Monday at 01:00 (Sun 17:00 UTC)")
    expect(timeWords(t, DEFAULT_TIME, taipei)).toBe("Every day at 11:00 (03:00 UTC)")
    expect(timeWords(t, DEFAULT_TIME, 0)).toBe("Every day at 03:00 (UTC)")
    expect(timeWords(translator(zhTW), sunday, taipei)).toBe("每週一 01:00（UTC 週日 17:00）")
  })

  test("a time input's value is minutes past midnight", () => {
    expect(clockMinute("07:30")).toBe(450)
    expect(clockMinute("23:59:00")).toBe(1439)
    expect(clockMinute("")).toBeUndefined()
  })
})
