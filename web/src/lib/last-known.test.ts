import { afterEach, describe, expect, test } from "bun:test"

import { lastKnown } from "@/lib/last-known"

const stored = (value: string) => {
  Object.assign(globalThis, {
    window: { localStorage: { getItem: () => value, setItem: () => {} } },
  })
}

afterEach(() => {
  Reflect.deleteProperty(globalThis, "window")
})

describe("what this browser last read", () => {
  test("a saved cluster comes back", () => {
    stored(`{"at":1,"cluster":"home"}`)
    expect(lastKnown()).toEqual({ at: 1, cluster: "home" })
  })

  test.each([
    ["not JSON", "{"],
    ["not an object", "3"],
    ["a field of the wrong type", `{"cluster":7}`],
    ["projects that are not a list", `{"projects":"shop"}`],
  ])("%s reads as nothing known", (_, value) => {
    stored(value)
    expect(lastKnown()).toEqual({})
  })
})
