import { afterAll, beforeAll, describe, expect, test } from "bun:test"

import { translator } from "@/i18n"
import { en } from "@/i18n/en"
import { formChanged, formOf, tagOf, updateOf, windowText } from "@/model/image-updates"

const now = new Date("2026-11-26T12:00:00Z")
const t = translator(en)

describe("in Taipei, UTC+8", () => {
  const zone = process.env.TZ
  beforeAll(() => {
    process.env.TZ = "Asia/Taipei"
  })
  afterAll(() => {
    process.env.TZ = zone
  })

  test("a local Sunday 03:00 is Saturday 19:00 UTC", () => {
    expect(updateOf({ policy: "patch", when: "weekly", day: 0, hour: 3 }, now)).toEqual({
      kind: "image",
      policy: "patch",
      window: { day: "saturday", hour: 19 },
    })
  })

  test("a stored window reads back as the local day and hour", () => {
    const updates = {
      allowed: ["patch" as const],
      setting: {
        kind: "image" as const,
        policy: "patch" as const,
        window: { day: "saturday" as const, hour: 19 },
      },
    }
    const form = formOf(updates, now)
    expect(form).toEqual({ policy: "patch", when: "weekly", day: 0, hour: 3 })
    expect(formChanged(updates, form, now)).toBe(false)
    expect(formChanged(updates, { ...form, hour: 4 }, now)).toBe(true)
  })

  test("a daily window has no day", () => {
    expect(updateOf({ policy: "minor", when: "daily", day: 2, hour: 9 }, now)).toEqual({
      kind: "image",
      policy: "minor",
      window: { day: undefined, hour: 1 },
    })
  })
})

test("checked every hour sends no window, and off turns it off", () => {
  expect(updateOf({ policy: "sameTag", when: "hourly", day: 0, hour: 3 }, now)).toEqual({
    kind: "image",
    policy: "sameTag",
  })
  expect(updateOf({ policy: "off", when: "weekly", day: 0, hour: 3 }, now)).toEqual({
    kind: "off",
  })
})

test("a setting written outside Zeabur reads as off, and saving replaces it", () => {
  const updates = { allowed: [], setting: { kind: "other" as const, policy: "glob:build-*" } }
  const form = formOf(updates, now)
  expect(form.policy).toBe("off")
  expect(formChanged(updates, form, now)).toBe(true)
})

test("an image's tag, with a registry port and a digest", () => {
  expect(tagOf("localhost:5000/redis:7.2.4@sha256:abc")).toBe("7.2.4")
  expect(tagOf("redis")).toBe("latest")
})

test("the hint names the UTC day and time", () => {
  expect(windowText(t, { day: "sunday", hour: 3 })).toBe("Sundays at 03:00 UTC")
  expect(windowText(t, { hour: 21 })).toBe("every day at 21:00 UTC")
})
