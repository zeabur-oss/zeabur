import { describe, expect, test } from "bun:test"

import {
  fixOf,
  missing,
  nameProblem,
  settingsOf,
  verdict,
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
    expect(missing(form({ kind: "webhook", values: {} }), true)).toEqual([])
    expect(missing(form({ kind: "webhook", values: {} }))).toEqual(["url"])
    expect(
      missing(form({ kind: "ntfy", values: { server: "https://ntfy.example.com" } }), true),
    ).toEqual(["topic"])
  })

  test("a rejected channel comes first in the verdict", () => {
    const channel = (status: "active" | "rejected" | "waiting") =>
      ({ status }) as Parameters<typeof verdict>[0][number]
    expect(verdict([])).toEqual({ as: "none" })
    expect(verdict([channel("active"), channel("rejected")])).toEqual({
      as: "rejected",
      count: 1,
    })
    expect(verdict([channel("waiting")])).toEqual({ as: "waiting", count: 1 })
  })

  test("the operator's reasons map to what to do", () => {
    expect(
      fixOf(
        'unable to fetch secret="zeabur-notify-ops", ns="shop": secrets "zeabur-notify-ops" not found',
      ),
    ).toBe("secretMissing")
    expect(
      fixOf('expected key="url" was not found at object="secret/shop/zeabur-notify-ops"'),
    ).toBe("secretMissing")
    expect(
      fixOf(
        'invalid URL (hidden) in key url from secret zeabur-notify-ops: unsupported scheme "ftp" for URL',
      ),
    ).toBe("scheme")
    expect(fixOf("invalid URL (hidden) in key url from secret x: missing host for URL")).toBe("url")
    expect(fixOf("required field 'chat_id' must be set")).toBe("required")
    expect(fixOf('smarthost="mail.example.com": port cannot be empty')).toBe("port")
    expect(fixOf("bad matcher")).toBeUndefined()
  })
})
