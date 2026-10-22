import { describe, expect, test } from "bun:test"

import { CHECK_EVERY, check, checkUrl, due } from "@/model/browser-check"

describe("a browser check of a public address", () => {
  test("checks https only", () => {
    expect(checkUrl("shop.example.com", true)).toBe("https://shop.example.com/")
    expect(checkUrl("shop.example.com", false)).toBeUndefined()
  })

  test("runs at most once a minute per address, never while one runs or when skipped", () => {
    expect(due(undefined, 0)).toBe(true)
    expect(due({ state: "checking" }, 0)).toBe(false)
    expect(due({ state: "skipped" }, CHECK_EVERY * 10)).toBe(false)
    expect(due({ state: "reachable", at: 1_000 }, 1_000 + CHECK_EVERY - 1)).toBe(false)
    expect(due({ state: "unreachable", at: 1_000 }, 1_000 + CHECK_EVERY)).toBe(true)
  })

  test("an opaque reply is reachable; a failure or no reply in time is not", async () => {
    let init: RequestInit | undefined
    const reply = await check(
      "https://shop.example.com/",
      async (_, i) => {
        init = i
        return { type: "opaque" }
      },
      () => 7,
    )
    expect(reply).toStrictEqual({ state: "reachable", at: 7 })
    expect(init).toMatchObject({ mode: "no-cors", cache: "no-store" })

    const failed = await check(
      "https://shop.example.com/",
      async () => {
        throw new TypeError("Failed to fetch")
      },
      () => 8,
    )
    expect(failed).toStrictEqual({ state: "unreachable", at: 8 })

    const silent = await check(
      "https://shop.example.com/",
      (_, i) =>
        new Promise((_resolve, reject) =>
          i.signal?.addEventListener("abort", () => reject(new DOMException("", "AbortError"))),
        ),
      () => 9,
      10,
    )
    expect(silent).toStrictEqual({ state: "unreachable", at: 9 })
  })
})
