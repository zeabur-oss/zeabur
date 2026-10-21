import { describe, expect, test } from "bun:test"

import { clusterNameOf, formatClock, personOf } from "@/lib/identity"

describe("identity", () => {
  test("an OIDC prefix is not the person", () => {
    expect(personOf("zeabur:yjpan@example.com")).toBe("yjpan@example.com")
    expect(personOf("oidc:issuer:ada")).toBe("ada")
    expect(personOf("ada@example.com")).toBe("ada@example.com")
    expect(personOf("system:serviceaccount:zeabur:zeabur")).toBe(
      "system:serviceaccount:zeabur:zeabur",
    )
  })

  test("an in-cluster address is no cluster name", () => {
    expect(clusterNameOf("10.43.0.1")).toBe(undefined)
    expect(clusterNameOf("[fd00::1]")).toBe(undefined)
    expect(clusterNameOf("kubernetes.default.svc")).toBe(undefined)
    expect(clusterNameOf("kubernetes")).toBe(undefined)
    expect(clusterNameOf("k3d-zeabur")).toBe("k3d-zeabur")
    expect(clusterNameOf("api.prod.example.com")).toBe("api.prod.example.com")
    expect(clusterNameOf(undefined)).toBe(undefined)
  })

  test("a time of day is 24-hour in Chinese", () => {
    const iso = "2026-10-21T07:04:00Z"
    const hour = new Date(iso).getHours()
    expect(formatClock(iso, "zh-TW")).toBe(`${String(hour).padStart(2, "0")}:04`)
    expect(formatClock(iso, "zh-CN")).toBe(`${String(hour).padStart(2, "0")}:04`)
    expect(formatClock(iso, "en")).toMatch(/^\d{1,2}:04\s?(AM|PM)$/)
  })
})
