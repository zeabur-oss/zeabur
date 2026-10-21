import { describe, expect, test } from "bun:test"

import { translator } from "@/i18n"
import { en } from "@/i18n/en"
import { zhCN } from "@/i18n/zh-CN"
import { zhTW } from "@/i18n/zh-TW"
import { updatedWords } from "@/lib/format"

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
