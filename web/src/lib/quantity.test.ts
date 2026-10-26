import { describe, expect, test } from "bun:test"

import { translator } from "@/i18n"
import { en } from "@/i18n/en"
import { zhTW } from "@/i18n/zh-TW"
import { plainAmount } from "@/lib/quantity"

describe("plainAmount", () => {
  test("writes Kubernetes amounts in plain units", () => {
    const t = translator(en)
    expect(plainAmount(t, "100m", "cpu", "en")).toBe("0.1 cores")
    expect(plainAmount(t, "1", "cpu", "en")).toBe("1 core")
    expect(plainAmount(t, "128Mi", "bytes", "en")).toBe("128 MiB")
    expect(plainAmount(t, "5Gi", "bytes", "en")).toBe("5 GiB")
    expect(plainAmount(t, "1536Mi", "bytes", "en")).toBe("1.5 GiB")
  })

  test("says cores as 核心 in Chinese", () => {
    expect(plainAmount(translator(zhTW), "250m", "cpu", "zh-TW")).toBe("0.25 核心")
  })

  test("leaves text that is no amount as it is", () => {
    expect(plainAmount(translator(en), "lots", "cpu", "en")).toBe("lots")
  })
})
