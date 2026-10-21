import { describe, expect, test } from "bun:test"

import { formatClock, personOf } from "@/lib/identity"

describe("identity", () => {
  test("an OIDC prefix is not the person", () => {
    expect(personOf("zeabur:yjpan@example.com")).toBe("yjpan@example.com")
    expect(personOf("oidc:issuer:ada")).toBe("ada")
    expect(personOf("ada@example.com")).toBe("ada@example.com")
    expect(personOf("system:serviceaccount:zeabur:zeabur")).toBe(
      "system:serviceaccount:zeabur:zeabur",
    )
  })

  test("a time of day is 24-hour in every language", () => {
    const iso = "2026-10-21T07:04:00Z"
    const hour = new Date(iso).getHours()
    expect(formatClock(iso, "zh-TW")).toBe(`${String(hour).padStart(2, "0")}:04`)
    expect(formatClock(iso, "zh-CN")).toBe(`${String(hour).padStart(2, "0")}:04`)
    expect(formatClock(iso, "en")).toBe(`${String(hour).padStart(2, "0")}:04`)
  })
})
