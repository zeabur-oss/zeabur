import { describe, test, expect } from "bun:test"

import { QueryClient, QueryObserver } from "@tanstack/react-query"

import type { Schemas } from "@/api/client"
import { followElevation, type Clock } from "@/auth/follow-elevation"

describe("following the rights", () => {
  const NOW = Date.parse("2027-01-15T08:00:00Z")
  const UNTIL = "2027-01-15T08:05:00Z"
  type Session = Schemas["SessionView"]
  const everyday: Session = { signsIn: true, elevation: "available" }
  const admin: Session = { signsIn: true, elevation: "active", elevatedUntil: UNTIL }

  /** A client with a project query, whose refetches the test counts, and a clock it runs. */
  function rig() {
    const client = new QueryClient()
    let fetches = 0
    // Observed, as a page shows it, so invalidating refetches it.
    const observer = new QueryObserver(client, {
      queryKey: ["project", "shop"],
      queryFn: () => ++fetches,
    })
    const stopObserving = observer.subscribe(() => {})
    const timers: { at: number; run: () => void }[] = []
    const clock: Clock = {
      now: () => NOW,
      after: (ms, run) => {
        const timer = { at: NOW + ms, run }
        timers.push(timer)
        return () => timers.splice(timers.indexOf(timer), 1)
      },
    }
    const stop = followElevation(client, clock)
    const session = (value: Session) => client.setQueryData(["session"], value)
    const settle = () => new Promise((resolve) => setTimeout(resolve, 10))
    return {
      client,
      timers,
      session,
      settle,
      fetches: () => fetches,
      stop: () => {
        stop()
        stopObserving()
      },
    }
  }

  test("the cache is fetched again when rights start and when they are dropped", async () => {
    const r = rig()
    await r.settle()
    const first = r.fetches()
    r.session(everyday)
    await r.settle()
    expect(r.fetches(), "the first answer changes nothing").toBe(first)
    r.session(everyday)
    await r.settle()
    expect(r.fetches(), "the same rights change nothing").toBe(first)
    r.session(admin)
    await r.settle()
    expect(r.fetches(), "elevation started").toBe(first + 1)
    r.session(everyday)
    await r.settle()
    expect(r.fetches(), "elevation dropped").toBe(first + 2)
    expect(r.timers.length).toBe(0)
    r.stop()
  })

  test("at the end of the rights the session is asked again, with nothing else in flight", async () => {
    const r = rig()
    r.session(everyday)
    r.session(admin)
    await r.settle()
    expect(r.timers.map((t) => t.at)).toStrictEqual([Date.parse(UNTIL)])
    let asked = 0
    r.client.setQueryDefaults(["session"], {
      staleTime: Infinity,
      queryFn: () => {
        asked += 1
        return everyday
      },
    })
    // An observed session query, as the sidebar keeps one.
    const observer = new QueryObserver(r.client, { queryKey: ["session"] })
    const unsubscribe = observer.subscribe(() => {})
    await r.settle()
    const before = r.fetches()
    asked = 0
    r.timers[0]?.run()
    await r.settle()
    expect(asked).toBe(1)
    expect(r.fetches(), "the rights ended, so the page is fetched again").toBe(before + 1)
    unsubscribe()
    r.stop()
  })
})
