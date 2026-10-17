// The terminal's connection, apart from the screen: the WebSocket address, the control frames the
// server sends (crates/engawa-api/src/pods/exec.rs documents the protocol) and the states the tab shows.
import type { Problem, Schemas } from "@/api/client"
import { isRefusal } from "@/model/problem"
import { roleYaml } from "@/model/rbac"

export type ServerFrame = Schemas["ServerFrame"]
export type ClientFrame = Schemas["ClientFrame"]

/** The close codes the server uses on its own: the credential expired, or nothing happened for
 * too long. Any other close without an `exit` or `error` frame first is a lost connection. */
const closeExpired = 4000
const closeIdle = 4001

export type TerminalState =
  | { phase: "connecting" }
  | { phase: "open"; container: string }
  | {
      phase: "exited"
      code?: number
      reason?: Schemas["ExitReason"]
      message?: string
    }
  | { phase: "error"; problem: Problem }
  | { phase: "closed"; why: "expired" | "idle" | "lost" }

export type TerminalEvent = { type: "frame"; frame: ServerFrame } | { type: "close"; code: number }

/** The state after `event`. An `exit` or `error` frame is the last word: the close that follows
 * it does not replace it. */
export function nextState(state: TerminalState, event: TerminalEvent): TerminalState {
  if (event.type === "frame") {
    const frame = event.frame
    switch (frame.type) {
      case "ready":
        return { phase: "open", container: frame.container }
      case "exit":
        return {
          phase: "exited",
          code: frame.code ?? undefined,
          reason: frame.reason ?? undefined,
          message: frame.message ?? undefined,
        }
      case "error":
        return { phase: "error", problem: frame.problem }
    }
  }
  if (state.phase === "exited" || state.phase === "error") return state
  if (event.code === closeExpired) return { phase: "closed", why: "expired" }
  if (event.code === closeIdle) return { phase: "closed", why: "idle" }
  return { phase: "closed", why: "lost" }
}

/**
 * Whether a terminal connects again on its own: the cluster refused it, and admin rights have
 * just become active. Whatever granted them (this tab's "Retry as admin", another refusal's
 * button, another browser tab), the refused terminal is tried with them.
 */
export function reconnectsWithRights(
  state: TerminalState,
  before: Schemas["ElevationState"] | undefined,
  now: Schemas["ElevationState"] | undefined,
): boolean {
  return (
    state.phase === "error" && isRefusal(state.problem) && before !== "active" && now === "active"
  )
}

/** A control frame from the server, or `undefined` for one this interface does not know (the
 * protocol lets either side add frames). */
export function parseFrame(text: string): ServerFrame | undefined {
  try {
    const value: unknown = JSON.parse(text)
    if (typeof value !== "object" || value === null || !("type" in value)) return undefined
    const type = (value as { type: unknown }).type
    return type === "ready" || type === "exit" || type === "error"
      ? (value as ServerFrame)
      : undefined
  } catch {
    return undefined
  }
}

/** The WebSocket address of a terminal in `container` of `pod`, on the page's own origin. */
export function terminalUrl(
  location: Pick<Location, "protocol" | "host">,
  project: string,
  pod: string,
  container: string | undefined,
): string {
  const scheme = location.protocol === "https:" ? "wss:" : "ws:"
  const query = container ? `?${new URLSearchParams({ container })}` : ""
  return `${scheme}//${location.host}/api/v1/projects/${encodeURIComponent(project)}/pods/${encodeURIComponent(pod)}/exec${query}`
}

/** A Role and RoleBinding that let `user` open terminals in `namespace`: `get` on pods, which the
 * server reads first, and `get` and `create` on `pods/exec`. The terminal reaches kube-apiserver
 * over a WebSocket, whose handshake is a GET (RFC 6455 §4.1), so RBAC checks `get` on `pods/exec`
 * where `kubectl exec` over SPDY needs `create`; granting both covers either
 * (https://kubernetes.io/docs/reference/access-authn-authz/authorization/#determine-the-request-verb). */
export function execRole(namespace: string, user: string): string {
  return roleYaml({
    name: "engawa-terminal",
    namespace,
    rules: [
      { apiGroup: "", resources: ["pods"], verbs: ["get"] },
      { apiGroup: "", resources: ["pods/exec"], verbs: ["get", "create"] },
    ],
    username: user,
  })
}
