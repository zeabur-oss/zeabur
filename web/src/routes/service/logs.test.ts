import { describe, test, expect } from "bun:test"

import { parseLine } from "@/routes/service/logs"

describe("parseLine", () => {
  test("splits a UTC timestamp off the line", () => {
    const line = parseLine("2026-11-23T10:42:12.165Z listening on :5432")
    expect(line.text).toBe("listening on :5432")
    expect(line.time).toBe(Date.parse("2026-11-23T10:42:12.165Z"))
  })

  test("splits a timestamp with an offset off the line", () => {
    const line = parseLine("2026-11-23T18:42:12.165669457+08:00 listening on :5432")
    expect(line.text).toBe("listening on :5432")
    expect(line.time).toBe(Date.parse("2026-11-23T10:42:12.165Z"))
  })
})
