// The only way the interface talks to Engawa's API: typed by the committed OpenAPI description
// (`bun run gen:api` writes ./schema.d.ts from crates/engawa-api/openapi.yaml), same origin, the
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

  constructor(status: number, problem: Problem) {
    super(problem.detail)
    this.name = "ApiError"
    this.status = status
    this.problem = problem
  }
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
    fix: "Check that Engawa is still running, then reload the page.",
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
  throw new ApiError(status, problem)
}

export function toProblem(error: unknown): Problem {
  if (error instanceof ApiError) return error.problem
  if (error instanceof Error) return localProblem(0, error.message)
  return localProblem(0, String(error))
}

export function isStatus(error: unknown, status: number): boolean {
  return error instanceof ApiError && error.status === status
}
