// "Retry as admin": when the cluster refuses something and the server offers elevation
// (`elevation-required`), the person confirms with their passkey in a window of its own, and what
// was refused is asked again with the short-lived admin token, which every request then carries.
//
// The window cannot report back through `window.opener`: this page sets
// Cross-Origin-Opener-Policy: same-origin, so the trip through Zeabur Identity severs it. The server's last
// page in the window posts its outcome on a BroadcastChannel, which reaches every page of this
// origin and no other; the page that asked listens before it opens the window, and takes the first
// message that reads as an outcome. The window is opened in the click's handler, or the browser
// blocks it.
import { queryClient } from "@/api/queries"
import { REQUEST_HEADERS } from "@/auth/session"
import { store } from "@/lib/stores"

/** How an elevation ended; the server's results, plus what happens on this page. */
export type Outcome =
  | { result: "done"; until?: string }
  | { result: "not-allowed" | "cancelled" | "failed" | "signed-out" }
  /** The browser blocked the window. */
  | { result: "blocked" }
  /** The person closed the waiting dialog. */
  | { result: "dismissed" }

const fromServer = new Set(["done", "not-allowed", "cancelled", "failed", "signed-out"])

/** A message from the elevation window, or undefined for anything else on the channel. */
export function outcomeOf(data: unknown): Outcome | undefined {
  if (typeof data !== "object" || data === null || !("result" in data)) return undefined
  const { result } = data
  if (typeof result !== "string" || !fromServer.has(result)) return undefined
  if (result === "done") {
    const until = "until" in data && typeof data.until === "string" ? data.until : undefined
    return { result, until }
  }
  return { result } as Outcome
}

/** What elevation needs from the browser; tests pass their own. */
export interface Ports {
  /** Opens the window at `url`; false when the browser blocked it. */
  open: (url: string) => boolean
  /** Listens for messages from the window; returns how to stop. */
  listen: (onMessage: (data: unknown) => void) => () => void
}

const CHANNEL = "zeabur-elevation"

const browserPorts: Ports = {
  open: (url) => window.open(url, "zeabur-elevate", "popup,width=520,height=700") !== null,
  listen: (onMessage) => {
    const channel = new BroadcastChannel(CHANNEL)
    channel.onmessage = (event: MessageEvent) => onMessage(event.data)
    return () => channel.close()
  },
}

/** Opens the elevation window and waits for its outcome; `cancel` stops waiting. */
export function requestElevation(ports: Ports): {
  outcome: Promise<Outcome>
  cancel: () => void
} {
  let settle: (outcome: Outcome) => void = () => {}
  const outcome = new Promise<Outcome>((resolve) => {
    settle = resolve
  })
  let done = false
  let stop = () => {}
  const finish = (result: Outcome) => {
    if (done) return
    done = true
    stop()
    settle(result)
  }
  stop = ports.listen((data) => {
    const read = outcomeOf(data)
    if (read) finish(read)
  })
  if (!ports.open("/auth/elevate")) finish({ result: "blocked" })
  return { outcome, cancel: () => finish({ result: "dismissed" }) }
}

/** What the elevation dialog shows. */
export type ElevationState =
  | { phase: "idle" }
  | { phase: "waiting"; cancel: () => void }
  | { phase: "ended"; outcome: Outcome; again: () => void }

export const elevation = store<ElevationState>({ phase: "idle" })

/**
 * Asks for admin rights, then runs `retry` (the same apply, a reconnect) once they are granted;
 * what the page reads is fetched again on its own (auth/follow-elevation.ts). Call it from a click
 * handler.
 */
export async function retryAsAdmin(
  retry: () => void = () => {},
  ports: Ports = browserPorts,
): Promise<void> {
  const { outcome, cancel } = requestElevation(ports)
  elevation.set({ phase: "waiting", cancel })
  const ended = await outcome
  switch (ended.result) {
    case "done":
      elevation.set({ phase: "idle" })
      void queryClient.invalidateQueries({ queryKey: ["session"] })
      retry()
      return
    case "dismissed":
      elevation.set({ phase: "idle" })
      return
    case "not-allowed":
    case "cancelled":
    case "failed":
    case "signed-out":
    case "blocked":
      elevation.set({
        phase: "ended",
        outcome: ended,
        again: () => void retryAsAdmin(retry, ports),
      })
  }
}

/** Drops admin rights before they expire. */
export async function dropElevation(): Promise<void> {
  await fetch("/auth/elevation", {
    method: "DELETE",
    headers: REQUEST_HEADERS,
    credentials: "same-origin",
  })
  await queryClient.invalidateQueries({ queryKey: ["session"] })
}
