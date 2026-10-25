import { describe, test, expect } from "bun:test"

import { ApiError, unwrap } from "@/api/client"
import { IDLE_AFTER_MS, isBackground, signInHref } from "@/auth/session"

describe("signing in again", () => {
  test("a signed-out answer sends the browser to sign in once, back to this page", () => {
    const assigned: string[] = []
    const location = {
      pathname: "/projects/shop",
      search: "",
      hash: "",
      assign: (href: string) => assigned.push(href),
    }
    Object.assign(globalThis, { window: { location } })
    const signedOut = {
      error: {
        type: "/problems/signed-out",
        title: "You are signed out",
        status: 401,
        detail: "Sign in to use Zeabur.",
        fix: "Sign in again.",
        kind: "signed-out",
      },
      response: new Response(null, { status: 401 }),
    }
    // Every request on the page fails at once; the browser leaves once.
    for (let i = 0; i < 3; i += 1) expect(() => unwrap(signedOut)).toThrow(ApiError)
    expect(assigned).toStrictEqual(["/auth/login?return=%2Fprojects%2Fshop"])

    // A credential the cluster refused is not a signed-out session.
    const refused = {
      ...signedOut,
      error: { ...signedOut.error, kind: "unauthenticated", type: "/problems/unauthenticated" },
    }
    expect(() => unwrap(refused)).toThrow(ApiError)
    expect(assigned.length).toBe(1)
  })

  test("comes back to the page the session ended on", () => {
    expect(signInHref({ pathname: "/projects/shop", search: "?tab=logs", hash: "" })).toBe(
      "/auth/login?return=%2Fprojects%2Fshop%3Ftab%3Dlogs",
    )
    expect(signInHref({ pathname: "/", search: "", hash: "" })).toBe("/auth/login")
  })
})

describe("background requests", () => {
  const input = 1_000_000

  test("a read becomes background work once the page has had no input for a minute", () => {
    expect(isBackground("GET", input, input + IDLE_AFTER_MS - 1)).toBe(false)
    expect(isBackground("GET", input, input + IDLE_AFTER_MS)).toBe(true)
  })

  test("a change is never background work, however long the page was left alone", () => {
    for (const method of ["POST", "PUT", "PATCH", "DELETE"])
      expect(isBackground(method, input, input + 10 * IDLE_AFTER_MS), method).toBe(false)
  })
})
