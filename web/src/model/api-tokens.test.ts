import { describe, expect, it } from "bun:test"

import { expired, validName } from "@/model/api-tokens"

describe("names", () => {
  it("takes what kube-apiserver takes as a ServiceAccount's name", () => {
    expect(validName("github-actions")).toBe(true)
    expect(validName("CI")).toBe(false)
    expect(validName("-ci")).toBe(false)
    expect(validName("a".repeat(64))).toBe(false)
  })
})

describe("expiry", () => {
  it("is past once its time has come, and unknown is not past", () => {
    const now = Date.parse("2026-11-06T00:00:00Z")
    expect(expired("2026-11-05T23:59:59Z", now)).toBe(true)
    expect(expired("2026-11-06T00:00:01Z", now)).toBe(false)
    expect(expired(undefined, now)).toBe(false)
  })
})
