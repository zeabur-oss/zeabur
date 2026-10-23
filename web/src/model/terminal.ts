// Protocol: crates/zeabur-api/src/pods/exec.rs.
import type { Problem, Schemas } from "@/api/client"
import { isRefusal } from "@/model/problem"
import { roleYaml } from "@/model/rbac"

export type ServerFrame = Schemas["ServerFrame"]
export type ClientFrame = Schemas["ClientFrame"]

// Any other close without an `exit` or `error` frame first is a lost connection.
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

export type TerminalEvent =
  | { type: "connect" }
  | { type: "frame"; frame: ServerFrame }
  | { type: "close"; code: number }

export function nextState(state: TerminalState, event: TerminalEvent): TerminalState {
  if (event.type === "connect") return { phase: "connecting" }
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

export function reconnectsWithRights(
  state: TerminalState,
  before: Schemas["ElevationState"] | undefined,
  now: Schemas["ElevationState"] | undefined,
): boolean {
  return (
    state.phase === "error" && isRefusal(state.problem) && before !== "active" && now === "active"
  )
}

/** `undefined` for an unknown frame: the protocol lets either side add frames. */
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

export function terminalUrl(
  location: Pick<Location, "protocol" | "host">,
  project: string,
  pod: string,
  container: string | undefined,
  command: string[] = [],
): string {
  const scheme = location.protocol === "https:" ? "wss:" : "ws:"
  const params = new URLSearchParams(container ? { container } : {})
  for (const arg of command) params.append("command", arg)
  const query = params.size > 0 ? `?${params}` : ""
  return `${scheme}//${location.host}/api/v1/projects/${encodeURIComponent(project)}/pods/${encodeURIComponent(pod)}/exec${query}`
}

export function toolsTerminalUrl(
  location: Pick<Location, "protocol" | "host">,
  project: string,
  name: string,
): string {
  const scheme = location.protocol === "https:" ? "wss:" : "ws:"
  return `${scheme}//${location.host}/api/v1/projects/${encodeURIComponent(project)}/tools/${encodeURIComponent(name)}/terminal`
}

/** The soonest of the credentials the open terminals were opened with: the first to close. */
export function nearestExpiry(expiries: Iterable<string | null | undefined>): string | undefined {
  let nearest: { at: number; iso: string } | undefined
  for (const iso of expiries) {
    if (!iso) continue
    const at = Date.parse(iso)
    if (Number.isNaN(at)) continue
    if (!nearest || at < nearest.at) nearest = { at, iso }
  }
  return nearest?.iso
}

/** Typed into a shell, a command waits for Enter: no line break after it. */
export function typedCommand(command: string): string {
  return command.replace(/\r\n?/g, "\n").trimEnd()
}

/** A whole object, with its own `apiVersion` and `kind`, not a fragment to merge into one. */
export function isManifest(yaml: string): boolean {
  return /^apiVersion:/m.test(yaml) && /^kind:/m.test(yaml)
}

/** `kubectl apply` with the YAML in a quoted here-document, so nothing in it expands; its end
 * marker is one no line of the YAML equals. */
export function applyCommand(yaml: string): string {
  return withDocument("kubectl apply -f -", yaml)
}

/** `kubectl create` in the project's namespace, which refuses a name that exists as Create does. */
export function createCommand(namespace: string, yaml: string): string {
  return withDocument(`kubectl create --namespace ${namespace} -f -`, yaml)
}

function withDocument(command: string, yaml: string): string {
  const lines = typedCommand(yaml).split("\n")
  let end = "EOF"
  for (let n = 2; lines.includes(end); n += 1) end = `EOF${n}`
  return [`${command} <<'${end}'`, ...lines, end].join("\n")
}

/** Both verbs on `pods/exec`: a WebSocket handshake is a GET (RFC 6455 §4.1), so RBAC checks `get`
 * where `kubectl exec` over SPDY needs `create`
 * (https://kubernetes.io/docs/reference/access-authn-authz/authorization/#determine-the-request-verb). */
export function execRole(namespace: string, user: string): string {
  return roleYaml({
    name: "zeabur-terminal",
    namespace,
    rules: [
      { apiGroup: "", resources: ["pods"], verbs: ["get"] },
      { apiGroup: "", resources: ["pods/exec"], verbs: ["get", "create"] },
    ],
    username: user,
  })
}
