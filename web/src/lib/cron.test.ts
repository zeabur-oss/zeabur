import { describe, expect, test } from "bun:test"

import { translator } from "@/i18n"
import { en } from "@/i18n/en"
import { zhCN } from "@/i18n/zh-CN"
import { zhTW } from "@/i18n/zh-TW"
import { describeCron, parseCron } from "@/lib/cron"

const t = translator(en)

describe("describeCron", () => {
  test("reads CloudNativePG's six fields, seconds first", () => {
    expect(describeCron(t, "0 0 3 * * *")).toBe("Every day at 03:00 (UTC)")
    expect(describeCron(t, "0 0 3 * * 0")).toBe("Every Sunday at 03:00 (UTC)")
  })

  test("reads five fields", () => {
    expect(describeCron(t, "0 3 * * *")).toBe("Every day at 03:00 (UTC)")
    expect(describeCron(t, "30 2 * * 1,3")).toBe("Every Monday and Wednesday at 02:30 (UTC)")
    expect(describeCron(t, "0 9 * * MON-FRI")).toBe("Monday to Friday at 09:00 (UTC)")
    expect(describeCron(t, "0 0,12 * * *")).toBe("Every day at 00:00 and 12:00 (UTC)")
    expect(describeCron(t, "15 4 1 * *")).toBe("On day 1 of every month at 04:15 (UTC)")
    expect(describeCron(t, "* * * * *")).toBe("Every minute (UTC)")
    expect(describeCron(t, "*/15 * * * *")).toBe("Every 15 minutes (UTC)")
    expect(describeCron(t, "0 * * * *")).toBe("Every hour (UTC)")
    expect(describeCron(t, "5 * * * *")).toBe("Every hour at minute 5 (UTC)")
    expect(describeCron(t, "0 */6 * * *")).toBe("Every 6 hours (UTC)")
  })

  test("Sunday is 0 or 7, and macros read as their fields", () => {
    expect(describeCron(t, "0 3 * * 7")).toBe("Every Sunday at 03:00 (UTC)")
    expect(describeCron(t, "@daily")).toBe("Every day at 00:00 (UTC)")
    expect(describeCron(t, "@weekly")).toBe("Every Sunday at 00:00 (UTC)")
  })

  test("names the zone written in front of the fields", () => {
    expect(describeCron(t, "CRON_TZ=Asia/Taipei 0 3 * * *")).toBe(
      "Every day at 03:00 (Asia/Taipei)",
    )
    expect(describeCron(t, "TZ=Europe/Berlin 0 0 3 * * *")).toBe(
      "Every day at 03:00 (Europe/Berlin)",
    )
  })

  test("leaves uncommon shapes as written", () => {
    for (const cron of [
      "30 0 3 * * *",
      "0 3 * 1 *",
      "0 3 1 * 1",
      "*/7 * * * *",
      "0 3-5 * * *",
      "0 0,6,12,18 * * *,1",
      "1,2,3 1,2 * * *",
      "@every 1h",
      "@yearly",
      "0 3 * *",
      "60 3 * * *",
      "",
    ]) {
      expect(parseCron(cron)).toBeUndefined()
      expect(describeCron(t, cron)).toBeUndefined()
    }
  })

  test("words it in Chinese", () => {
    expect(describeCron(translator(zhTW), "0 0 3 * * *")).toBe("每天 03:00（UTC）")
    expect(describeCron(translator(zhCN), "0 0 3 * * *")).toBe("每天 03:00（UTC）")
    expect(describeCron(translator(zhTW), "0 9 * * 1-5")).toBe("每週一至週五 09:00（UTC）")
    expect(describeCron(translator(zhCN), "0 3 * * 0")).toBe("每周日 03:00（UTC）")
    expect(describeCron(translator(zhTW), "0 3 1 * *")).toBe("每月 1 日 03:00（UTC）")
    expect(describeCron(translator(zhTW), "*/15 * * * *")).toBe("每 15 分鐘（UTC）")
  })
})
