import { afterEach, describe, expect, jest, test } from "bun:test"
import { QueryClient } from "@tanstack/react-query"
import { createElement } from "react"
import { renderToStaticMarkup } from "react-dom/server"

import {
  answer,
  ApiError,
  localProblem,
  retryAfterSeconds,
  type Problem as ProblemBody,
} from "@/api/client"
import { queryClient, retryDelay, retryRead } from "@/api/queries"
import { Problem } from "@/components/problem"

/** A refusal as Zeabur sends it: the problem as the body. */
function refused(status: number, headers: Record<string, string> = {}, problem?: ProblemBody) {
  return new Response(JSON.stringify(problem ?? localProblem(status, "refused")), {
    status,
    headers: { "content-type": "application/problem+json", ...headers },
  })
}

async function caught(answered: Promise<unknown>): Promise<ApiError> {
  try {
    await answered
  } catch (error) {
    if (error instanceof ApiError) return error
    throw error
  }
  throw new Error("no ApiError thrown")
}

describe("Retry-After", () => {
  const now = Date.parse("Fri, 31 Dec 1999 23:59:00 GMT")

  test("a number of seconds", () => {
    expect(retryAfterSeconds("120", now)).toBe(120)
    expect(retryAfterSeconds(" 0 ", now)).toBe(0)
  })

  test("an HTTP-date, counted from now and never below 0", () => {
    expect(retryAfterSeconds("Fri, 31 Dec 1999 23:59:59 GMT", now)).toBe(59)
    expect(retryAfterSeconds("Fri, 31 Dec 1999 23:00:00 GMT", now)).toBe(0)
  })

  test("anything else is no answer", () => {
    expect(retryAfterSeconds(null, now)).toBeUndefined()
    expect(retryAfterSeconds("", now)).toBeUndefined()
    expect(retryAfterSeconds("soon", now)).toBeUndefined()
    expect(retryAfterSeconds("-5", now)).toBeUndefined()
  })

  test("a 429 or 503 carries the wait, the header first", async () => {
    expect((await caught(answer(refused(429, { "Retry-After": "3" })))).retryAfter).toBe(3)
    const status = {
      ...localProblem(503, "busy"),
      kubernetes: {
        code: 503,
        reason: "ServiceUnavailable",
        message: "busy",
        retryAfterSeconds: 7,
      },
    }
    expect((await caught(answer(refused(503, {}, status)))).retryAfter).toBe(7)
    expect((await caught(answer(refused(503, { "Retry-After": "2" }, status)))).retryAfter).toBe(2)
  })

  test("other answers carry none", async () => {
    expect((await caught(answer(refused(500, { "Retry-After": "3" })))).retryAfter).toBeUndefined()
    expect((await caught(answer(refused(404)))).retryAfter).toBeUndefined()
  })
})

describe("retrying a read", () => {
  const error = (status: number, retryAfter?: number) =>
    new ApiError(status, localProblem(status, "x"), retryAfter)

  test("429 and 503 are tried again, other 4xx are not, at most twice", () => {
    expect(retryRead(0, error(429))).toBe(true)
    expect(retryRead(0, error(503))).toBe(true)
    expect(retryRead(0, error(403))).toBe(false)
    expect(retryRead(2, error(503))).toBe(false)
  })

  test("waits all the server asked, else backs off up to 30 s", () => {
    expect(retryDelay(0, error(429, 3))).toBe(3_000)
    expect(retryDelay(0, error(503, 120))).toBe(120_000)
    expect(retryDelay(1, error(503, 120))).toBe(120_000)
    expect(retryDelay(0, error(503))).toBe(1_000)
    expect(retryDelay(1, new Error("offline"))).toBe(2_000)
    expect(retryDelay(10, new Error("offline"))).toBe(30_000)
  })

  test("a wait longer than setTimeout keeps is the longest it keeps, not an instant retry", () => {
    expect(retryDelay(0, error(503, 10 ** 9))).toBe(2 ** 31 - 1)
  })
})

describe("the query client", () => {
  afterEach(() => jest.useRealTimers())

  /** Lets the promise callbacks queued by a failed try run. */
  async function settle() {
    for (let i = 0; i < 20; i += 1) await Promise.resolve()
  }

  test("a 429 with Retry-After: 3 waits 3 s before the retry", async () => {
    jest.useFakeTimers()
    const client = new QueryClient({ defaultOptions: queryClient.getDefaultOptions() })
    let tries = 0
    const read = client.query({
      queryKey: ["limited"],
      queryFn: () => {
        tries += 1
        if (tries === 1) return answer<string>(refused(429, { "Retry-After": "3" }))
        return "ok"
      },
    })
    await settle()
    expect(tries).toBe(1)
    jest.advanceTimersByTime(2_999)
    await settle()
    expect(tries).toBe(1)
    jest.advanceTimersByTime(1)
    await settle()
    expect(tries).toBe(2)
    expect(await read).toBe("ok")
    client.clear()
  })
})

describe("the problem shown", () => {
  const shown = (error: ApiError) => renderToStaticMarkup(createElement(Problem, { error }))

  test("says how long the cluster asked to wait", () => {
    expect(shown(new ApiError(429, localProblem(429, "busy"), 3))).toContain(
      "The cluster asked to wait 3 seconds.",
    )
    expect(shown(new ApiError(503, localProblem(503, "busy"), 120))).toContain(
      "The cluster asked to wait 2 minutes.",
    )
    expect(shown(new ApiError(503, localProblem(503, "busy"), 90))).toContain(
      "The cluster asked to wait 2 minutes.",
    )
    expect(shown(new ApiError(503, localProblem(503, "busy"), 7200))).toContain(
      "The cluster asked to wait 2 hours.",
    )
    expect(shown(new ApiError(503, localProblem(503, "busy")))).not.toContain("asked to wait")
  })
})
