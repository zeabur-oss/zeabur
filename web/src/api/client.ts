// The only way the interface talks to Zeabur's API: typed by the committed OpenAPI description
// (`bun run gen:api` writes ./schema.d.ts from crates/zeabur-api/openapi.yaml), same origin, the
// session cookie sent by the browser, and the header that marks a request as the interface's own.
import createClient from "openapi-fetch"

import type { components, paths } from "@/api/schema"
import { goToSignIn, REQUEST_HEADERS } from "@/auth/session"

export type Schemas = components["schemas"]
export type Problem = Schemas["Problem"]

export const api = createClient<paths>({ credentials: "same-origin", headers: REQUEST_HEADERS })

/** An API answer that was not a success, carrying the server's problem (RFC 9457). */
export class ApiError extends Error {
  readonly status: number
  readonly problem: Problem
  /** How many seconds the server asked to wait before trying again; set on a 429 or 503 only. */
  readonly retryAfter: number | undefined

  constructor(status: number, problem: Problem, retryAfter?: number) {
    super(problem.detail)
    this.name = "ApiError"
    this.status = status
    this.problem = problem
    this.retryAfter = retryAfter
  }
}

/** The seconds a `Retry-After` value asks to wait: a number of seconds or an HTTP-date (RFC 9110,
 * section 10.2.3), a date already past being 0. Anything else is no answer. */
export function retryAfterSeconds(value: string | null, now = Date.now()): number | undefined {
  const text = value?.trim()
  if (!text) return undefined
  if (/^\d+$/.test(text)) return Number(text)
  // Every HTTP-date format starts with the day's name (section 5.6.7); `Date.parse` alone would
  // read `-5` as a year.
  if (!/^(Mon|Tue|Wed|Thu|Fri|Sat|Sun)/.test(text)) return undefined
  const date = Date.parse(text)
  if (Number.isNaN(date)) return undefined
  return Math.max(0, Math.ceil((date - now) / 1000))
}

function isProblem(value: unknown): value is Problem {
  return (
    typeof value === "object" &&
    value !== null &&
    "title" in value &&
    "detail" in value &&
    "fix" in value
  )
}

/** A problem for answers that carry none (a proxy error, a dropped connection). */
export function localProblem(status: number, detail: string): Problem {
  const kind = status === 0 ? "unavailable" : status >= 500 ? "internal" : "bad-request"
  return {
    type: "about:blank",
    title: status === 0 ? "No answer" : `HTTP ${status}`,
    status,
    detail,
    fix: "Check that Zeabur is still running, then reload the page.",
    kind,
    summary: { key: `problem.summary.${kind}` },
  }
}

/** Returns `data`, or throws an `ApiError` with the server's problem. A session that ended sends
 * the browser to sign in again. */
export function unwrap<T>(result: { data?: T; error?: unknown; response: Response }): T {
  if (result.data !== undefined && result.response.ok) return result.data
  const status = result.response.status
  const problem = isProblem(result.error)
    ? result.error
    : localProblem(status, `The server answered ${status} ${result.response.statusText}.`)
  if (problem.kind === "signed-out") goToSignIn()
  // Too many requests or unavailable for now (RFC 6585, section 4; RFC 9110, section 15.6.4): the
  // wait the header asks for, else the one kube-apiserver put in its Status.
  const retryAfter =
    status === 429 || status === 503
      ? (retryAfterSeconds(result.response.headers.get("retry-after")) ??
        problem.kubernetes?.retryAfterSeconds ??
        undefined)
      : undefined
  throw new ApiError(status, problem, retryAfter)
}

export function toProblem(error: unknown): Problem {
  if (error instanceof ApiError) return error.problem
  if (error instanceof Error) return localProblem(0, error.message)
  return localProblem(0, String(error))
}

export function isStatus(error: unknown, status: number): boolean {
  return error instanceof ApiError && error.status === status
}
