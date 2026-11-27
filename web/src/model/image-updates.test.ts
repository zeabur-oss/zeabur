import { afterAll, beforeAll, describe, expect, test } from "bun:test"

import type { Schemas } from "@/api/client"

import { translator } from "@/i18n"
import { en } from "@/i18n/en"
import {
  formOf,
  readKeelUpdates,
  localMinute,
  policyChoices,
  policyHint,
  reachText,
  tagOf,
  updateOf,
  windowText,
} from "@/model/image-updates"

const now = new Date("2026-11-26T12:00:00Z")
const t = translator(en)

test("CloudNativePG keeps custom Keel settings until explicitly replaced", () => {
  const cases: Record<string, string>[] = [
    { "keel.sh/policy": "patch", "keel.sh/pollSchedule": "@hourly" },
    {
      "keel.sh/policy": "force",
      "keel.sh/match-tag": "true",
      "keel.sh/trigger": "poll",
      "keel.sh/pollSchedule": "@every 5m",
    },
    {
      "keel.sh/policy": "force",
      "keel.sh/match-tag": "true",
      "keel.sh/trigger": "poll",
      "keel.sh/pollSchedule": "0 0 24 * * 0",
    },
  ]
  for (const annotations of cases) {
    const updates = readKeelUpdates(annotations, ["sameTag"])
    expect(updates.setting?.kind).toBe("other")
    expect(updateOf(updates, formOf(updates, now), now)).toBeUndefined()
  }
})

describe("in Taipei, UTC+8", () => {
  const zone = process.env.TZ
  beforeAll(() => {
    process.env.TZ = "Asia/Taipei"
  })
  afterAll(() => {
    process.env.TZ = zone
  })

  test("a local Sunday 03:00 is Saturday 19:00 UTC", () => {
    expect(updateOf(undefined, { policy: "patch", when: "weekly", day: 0, hour: 3 }, now)).toEqual({
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
    expect(updateOf(updates, { ...form, hour: 4 }, now)).toEqual({
      kind: "image",
      policy: "patch",
      window: { day: "saturday", hour: 20 },
    })
  })

  test("a daily window has no day", () => {
    expect(updateOf(undefined, { policy: "minor", when: "daily", day: 2, hour: 9 }, now)).toEqual({
      kind: "image",
      policy: "minor",
      window: { day: undefined, hour: 1 },
    })
  })
})

describe("in India, UTC+5:30", () => {
  const zone = process.env.TZ
  beforeAll(() => {
    process.env.TZ = "Asia/Kolkata"
  })
  afterAll(() => {
    process.env.TZ = zone
  })

  test("hours start at :30 and land on a whole UTC hour, and read back the same", () => {
    expect(localMinute(now)).toBe(30)
    const form = {
      policy: "patch" as const,
      when: "weekly" as const,
      day: 0,
      hour: 2,
    }
    const update = updateOf(undefined, form, now)
    expect(update).toEqual({
      kind: "image",
      policy: "patch",
      window: { day: "saturday", hour: 21 },
    })
    const updates = {
      allowed: ["patch" as const],
      setting: update as Schemas.UpdateSetting,
    }
    expect(formOf(updates, now)).toEqual(form)
  })
})

test("checked every hour sends no window, and off turns it off", () => {
  expect(updateOf(undefined, { policy: "sameTag", when: "hourly", day: 0, hour: 3 }, now)).toEqual({
    kind: "image",
    policy: "sameTag",
  })
  expect(updateOf(undefined, { policy: "off", when: "weekly", day: 0, hour: 3 }, now)).toEqual({
    kind: "off",
  })
})

test("every stored setting reads into the form and saves back unchanged", () => {
  expect(updateOf(undefined, formOf(undefined, now), now)).toEqual({ kind: "off" })
  const settings: Extract<Schemas.UpdateSetting, { kind: "image" }>[] = [
    { kind: "image", policy: "sameTag" },
    { kind: "image", policy: "minor", window: { hour: 21 } },
    { kind: "image", policy: "patch", window: { day: "wednesday", hour: 0 } },
  ]
  for (const setting of settings) {
    const updates = { allowed: ["patch" as const], setting }
    expect(updateOf(updates, formOf(updates, now), now)).toEqual(setting)
  }
})

test("a setting written outside Zeabur stays until another option replaces it", () => {
  const updates = {
    allowed: [],
    setting: { kind: "other" as const, policy: "glob:build-*" },
  }
  const form = formOf(updates, now)
  expect(form.policy).toBe("other")
  expect(policyHint(t, updates, form, "7.2.4")).toBe(
    "Keel updates this service with its glob:build-* policy, set outside Zeabur. Pick another option to replace it.",
  )
  expect(updateOf(updates, form, now)).toBeUndefined()
  expect(updateOf(updates, { ...form, policy: "off" }, now)).toEqual({ kind: "off" })
})

test("new builds left on a service no longer built from source can be turned off", () => {
  const updates = { allowed: ["patch" as const], setting: { kind: "newBuilds" as const } }
  const form = formOf(updates, now)
  expect(form.policy).toBe("other")
  expect(updateOf(updates, form, now)).toBeUndefined()
  expect(updateOf(updates, { ...form, policy: "off" }, now)).toEqual({ kind: "off" })
})

test("an image's tag, with a registry port and a digest", () => {
  expect(tagOf("localhost:5000/redis:7.2.4@sha256:abc")).toBe("7.2.4")
  expect(tagOf("redis")).toBe("latest")
})

test("the hint names the UTC day and time", () => {
  expect(windowText(t, { day: "sunday", hour: 3 })).toBe("Sundays at 03:00 UTC")
  expect(windowText(t, { hour: 21 })).toBe("every day at 21:00 UTC")
})

test("the hint names the versions a policy takes from the running tag", () => {
  expect(reachText(t, "7.2.4", "patch")).toBe(
    "7.2.4 moves to newer 7.2.x versions. 7.3 and later wait for you.",
  )
  expect(reachText(t, "v1.27", "minor")).toBe(
    "v1.27 moves to newer 1.x versions. 2.0 and later wait for you. After an update, the tag names the full version instead of v1.27.",
  )
  expect(reachText(t, "8", "patch")).toContain("8 moves to newer 8.0.x versions. 8.1 and later")
  expect(reachText(t, "latest", "major")).toBe(
    "latest moves to the newest version tag first, then to every newer version.",
  )
})

describe("in Los Angeles, the week daylight saving ends", () => {
  const zone = process.env.TZ
  beforeAll(() => {
    process.env.TZ = "America/Los_Angeles"
  })
  afterAll(() => {
    process.env.TZ = zone
  })

  test("changing only the policy keeps the stored window", () => {
    const week = new Date("2026-11-05T12:00:00Z")
    const updates = {
      allowed: ["patch" as const, "minor" as const],
      setting: {
        kind: "image" as const,
        policy: "patch" as const,
        window: { day: "sunday" as const, hour: 1 },
      },
    }
    const form = formOf(updates, week)
    expect(updateOf(updates, form, week)).toEqual(updates.setting)
    expect(updateOf(updates, { ...form, policy: "minor" }, week)).toEqual({
      kind: "image",
      policy: "minor",
      window: { day: "sunday", hour: 1 },
    })
  })
})

test("a digest-pinned image with updates on can still turn them off", () => {
  const updates = {
    allowed: [],
    setting: { kind: "image" as const, policy: "minor" as const },
  }
  expect(policyChoices(updates)).toEqual(["minor"])
  const form = formOf(updates, now)
  expect(updateOf(updates, { ...form, policy: "off" }, now)).toEqual({
    kind: "off",
  })
})
