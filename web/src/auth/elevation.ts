// Cross-Origin-Opener-Policy: same-origin severs `window.opener` on the trip through Zeabur
// Identity, so the window's last page posts its outcome on a BroadcastChannel.
import * as z from "zod/mini"

import { queryClient } from "@/api/queries"
import { REQUEST_HEADERS } from "@/auth/session"
import { store } from "@/lib/stores"

const fromServer = z.union([
  z.object({ result: z.literal("done"), until: z.catch(z.optional(z.string()), undefined) }),
  z.object({ result: z.enum(["not-allowed", "cancelled", "failed", "signed-out"]) }),
])

export type Outcome = z.output<typeof fromServer> | { result: "blocked" } | { result: "dismissed" }

export function outcomeOf(data: unknown): Outcome | undefined {
  return fromServer.safeParse(data).data
}

export interface Ports {
  /** False when the browser blocked the window. */
  open: (url: string) => boolean
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

export type ElevationState =
  | { phase: "idle" }
  | { phase: "waiting"; cancel: () => void }
  | { phase: "ended"; outcome: Outcome; again: () => void }

export const elevation = store<ElevationState>({ phase: "idle" })

/** Call from a click handler: the browser blocks a window opened later. */
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

export async function dropElevation(): Promise<void> {
  await fetch("/auth/elevation", {
    method: "DELETE",
    headers: REQUEST_HEADERS,
    credentials: "same-origin",
  })
  await queryClient.invalidateQueries({ queryKey: ["session"] })
}
