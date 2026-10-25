import { describe, expect, test } from "bun:test"

import { personOf } from "@/lib/identity"

describe("identity", () => {
  test("an OIDC prefix is not the person", () => {
    expect(personOf("zeabur:yjpan@example.com")).toBe("yjpan@example.com")
    expect(personOf("oidc:issuer:ada")).toBe("ada")
    expect(personOf("ada@example.com")).toBe("ada@example.com")
    expect(personOf("system:serviceaccount:zeabur:zeabur")).toBe(
      "system:serviceaccount:zeabur:zeabur",
    )
  })
})
