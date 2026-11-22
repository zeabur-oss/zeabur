import { describe, expect, test } from "bun:test"

import { translator } from "@/i18n"
import { en } from "@/i18n/en"
import { zhCN } from "@/i18n/zh-CN"
import { zhTW } from "@/i18n/zh-TW"
import { formatTime, updatedWords } from "@/lib/format"

const at = Date.UTC(2026, 9, 21, 12)
const later = (seconds: number) => at + seconds * 1000

describe("updatedWords", () => {
  test("says just now for the first minute, then minutes, hours and days", () => {
    const t = translator(en)
    expect(updatedWords(t, at, later(5))).toBe("Updated just now")
    expect(updatedWords(t, at, later(180))).toBe("Updated 3m ago")
    expect(updatedWords(t, at, later(7_200))).toBe("Updated 2h ago")
    expect(updatedWords(t, at, later(172_800))).toBe("Updated 2d ago")
  })

  test("puts the time first in Chinese", () => {
    expect(updatedWords(translator(zhTW), at, later(0))).toBe("剛剛更新")
    expect(updatedWords(translator(zhTW), at, later(180))).toBe("3 分鐘前更新")
    expect(updatedWords(translator(zhCN), at, later(0))).toBe("刚刚更新")
    expect(updatedWords(translator(zhCN), at, later(180))).toBe("3 分钟前更新")
  })

  test("a clock behind the update still reads as just now", () => {
    expect(updatedWords(translator(en), at, later(-30))).toBe("Updated just now")
  })
})

describe("formatTime", () => {
  const nearMidnight = "2026-11-22T23:30:00Z"

  test("names the zone, so a time reads beside a schedule in UTC", () => {
    const utc = formatTime(nearMidnight, "en-US", "UTC")
    expect(utc).toContain("Nov 22, 2026")
    expect(utc).toContain("11:30 PM UTC")
    const taipei = formatTime(nearMidnight, "en-US", "Asia/Taipei")
    expect(taipei).toContain("Nov 23, 2026")
    expect(taipei).toContain("7:30 AM GMT+8")
  })

  test("keeps the date and the zone together across midnight in Chinese", () => {
    const taipei = formatTime(nearMidnight, "zh-TW", "Asia/Taipei")
    expect(taipei).toContain("23")
    expect(taipei).toContain("GMT+8")
  })

  test("an unreadable time stays as written", () => {
    expect(formatTime("soon", "en-US")).toBe("soon")
  })
})
