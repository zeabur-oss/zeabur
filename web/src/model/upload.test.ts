import { describe, expect, test } from "bun:test"

import type { Build } from "@/model/builds"
import { newRequest, waitForToken } from "@/model/upload"

const build = (uploadRequest?: string) => ({ name: "site", uploadRequest }) as Build

describe("upload", () => {
  test("each request is new and short enough for the Build to answer", () => {
    const a = newRequest(1_800_000_000_000)
    const b = newRequest(1_800_000_000_000, () => 0.5)
    expect(a).not.toBe(b)
    for (const request of [a, b]) expect(request).toMatch(/^[a-z0-9]{1,63}$/)
  })

  test("it waits for the Build to answer this request, not an older one", async () => {
    const answers = [build(), build("old"), build("u1")]
    let read = 0
    const found = await waitForToken(async () => answers[read++], "u1", {
      interval: 1,
      sleep: async () => undefined,
    })
    expect(found).toBe(true)
    expect(read).toBe(3)
  })

  test("it gives up when the Build never answers", async () => {
    const found = await waitForToken(async () => build("old"), "u1", {
      timeout: 5,
      interval: 1,
      sleep: async () => undefined,
    })
    expect(found).toBe(false)
  })
})
