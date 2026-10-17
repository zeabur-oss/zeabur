// The page's data follows the person's rights: when admin rights start, are dropped or end, what
// was read with the others is fetched again (auth/elevation.ts asks for the rights). Kept apart
// from the elevation window so the first page load carries only this.
import type { QueryClient } from "@tanstack/react-query"

import type { Schemas } from "@/api/client"

type Session = Schemas["SessionView"]

/** The rights a session's answer stands for: everyday ones, or admin ones until a time. */
function rightsOf(session: Session): string {
  return session.elevation === "active" ? `active ${session.elevatedUntil ?? ""}` : "everyday"
}

/** Timers, so tests can run the clock. */
export interface Clock {
  now: () => number
  after: (ms: number, run: () => void) => () => void
}

const browserClock: Clock = {
  now: () => Date.now(),
  after: (ms, run) => {
    const id = window.setTimeout(run, ms)
    return () => window.clearTimeout(id)
  },
}

/**
 * Keeps what is on screen in step with the rights it was read with. Whenever the session's answer
 * says the rights changed (elevation started, dropped or ended), every other query is refetched:
 * data only an administrator may see does not stay, and what was refused before appears without a
 * reload. At `elevatedUntil` the session is asked again, also when nothing else is in flight, so
 * the end is noticed on time. Returns how to stop.
 */
export function followElevation(client: QueryClient, clock: Clock = browserClock): () => void {
  let rights: string | undefined
  let cancelTimer = () => {}
  const unsubscribe = client.getQueryCache().subscribe((event) => {
    if (event.type !== "updated" || event.action.type !== "success") return
    if (event.query.queryKey[0] !== "session") return
    const session = event.query.state.data as Session | undefined
    if (!session) return
    const now = rightsOf(session)
    if (now === rights) return
    const changed = rights !== undefined
    rights = now
    cancelTimer()
    cancelTimer = () => {}
    if (session.elevation === "active" && session.elevatedUntil) {
      const left = Date.parse(session.elevatedUntil) - clock.now()
      cancelTimer = clock.after(Math.max(0, left), () => {
        void client.invalidateQueries({ queryKey: ["session"] })
      })
    }
    if (changed) void client.invalidateQueries({ predicate: (q) => q.queryKey[0] !== "session" })
  })
  return () => {
    cancelTimer()
    unsubscribe()
  }
}
