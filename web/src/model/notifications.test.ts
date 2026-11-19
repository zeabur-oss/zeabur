import { describe, expect, test } from "bun:test"

import {
  missing,
  missingInEdit,
  nameProblem,
  settingsOf,
  type ChannelForm,
} from "@/model/notifications"

const form = (patch: Partial<ChannelForm>): ChannelForm => ({
  name: "ops",
  kind: "slack",
  values: {},
  events: ["deploy-failed"],
  ...patch,
})

describe("notification channels", () => {
  test("names follow the API's rule", () => {
    expect(nameProblem("ops-2")).toBe(false)
    expect(nameProblem("2ops")).toBe(true)
    expect(nameProblem("ops-")).toBe(true)
    expect(nameProblem("a".repeat(31))).toBe(true)
  })

  test("a Telegram chat needs a number and optional fields may stay empty", () => {
    expect(missing(form({ kind: "telegram", values: { botToken: "1:a", chatId: "x" } }))).toEqual([
      "chatId",
    ])
    expect(missing(form({ kind: "ntfy", values: { topic: "alerts" } }))).toEqual([])
    expect(
      settingsOf(form({ kind: "telegram", values: { botToken: " 1:a ", chatId: "-100" } })),
    ).toEqual({ kind: "telegram", botToken: "1:a", chatId: -100 })
  })

  test("an edit keeps stored fields left empty, but ntfy's topic goes with its server", () => {
    expect(missingInEdit(form({ kind: "webhook", values: {} }))).toEqual([])
    expect(missing(form({ kind: "webhook", values: {} }))).toEqual(["url"])
    expect(
      missingInEdit(form({ kind: "ntfy", values: { server: "https://ntfy.example.com" } })),
    ).toEqual(["topic"])
  })
})
