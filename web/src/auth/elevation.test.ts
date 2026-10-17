import { describe, test, expect } from "bun:test"

import { elevation, outcomeOf, requestElevation, retryAsAdmin, type Ports } from "@/auth/elevation"

/** A browser whose window and channel the test drives. */
function fakePorts({ blocked = false } = {}) {
  const opened: string[] = []
  let deliver: ((data: unknown) => void) | undefined
  let closed = false
  const ports: Ports = {
    open: (url) => {
      opened.push(url)
      return !blocked
    },
    listen: (onMessage) => {
      deliver = onMessage
      return () => {
        closed = true
      }
    },
  }
  return {
    ports,
    opened,
    post: (data: unknown) => deliver?.(data),
    closed: () => closed,
  }
}

describe("elevation messages", () => {
  test("only the server's outcomes are read", () => {
    expect(outcomeOf({ result: "done", until: "2027-01-15T08:05:00Z" })).toStrictEqual({
      result: "done",
      until: "2027-01-15T08:05:00Z",
    })
    expect(outcomeOf({ result: "not-allowed" })).toStrictEqual({ result: "not-allowed" })
    for (const other of [
      null,
      "done",
      { result: "dismissed" },
      { result: "blocked" },
      { result: 1 },
      { type: "something else" },
    ])
      expect(outcomeOf(other)).toBe(undefined)
  })
})

describe("the elevation window", () => {
  test("listens before it opens the window, and ends on the first outcome", async () => {
    const fake = fakePorts()
    const { outcome } = requestElevation(fake.ports)
    expect(fake.opened).toStrictEqual(["/auth/elevate"])
    fake.post({ unrelated: true })
    fake.post({ result: "done", until: "2027-01-15T08:05:00Z" })
    fake.post({ result: "failed" })
    expect(await outcome).toStrictEqual({ result: "done", until: "2027-01-15T08:05:00Z" })
    expect(fake.closed()).toBe(true)
  })

  test("a blocked window ends at once", async () => {
    const fake = fakePorts({ blocked: true })
    expect(await requestElevation(fake.ports).outcome).toStrictEqual({ result: "blocked" })
    expect(fake.closed()).toBe(true)
  })

  test("closing the dialog stops waiting", async () => {
    const fake = fakePorts()
    const { outcome, cancel } = requestElevation(fake.ports)
    cancel()
    fake.post({ result: "done" })
    expect(await outcome).toStrictEqual({ result: "dismissed" })
  })
})

describe("retry as admin", () => {
  test("granted rights retry the same change once", async () => {
    const fake = fakePorts()
    let retries = 0
    const asked = retryAsAdmin(() => (retries += 1), fake.ports)
    expect(elevation.get().phase).toBe("waiting")
    fake.post({ result: "done" })
    await asked
    expect(retries).toBe(1)
    expect(elevation.get().phase).toBe("idle")
  })

  test("a refusal is shown, nothing is retried, and asking again opens a new window", async () => {
    const fake = fakePorts()
    let retries = 0
    const asked = retryAsAdmin(() => (retries += 1), fake.ports)
    fake.post({ result: "not-allowed" })
    await asked
    const state = elevation.get()
    expect(state.phase).toBe("ended")
    if (state.phase !== "ended") return
    expect(state.outcome).toStrictEqual({ result: "not-allowed" })
    expect(retries).toBe(0)

    state.again()
    expect(fake.opened.length).toBe(2)
    expect(elevation.get().phase).toBe("waiting")
    const waiting = elevation.get()
    if (waiting.phase === "waiting") waiting.cancel()
  })
})
