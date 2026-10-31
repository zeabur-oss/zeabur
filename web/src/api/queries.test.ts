import { afterEach, expect, setSystemTime, spyOn, test } from "bun:test"
import { QueryClient } from "@tanstack/react-query"

import { ZEABUR_EVERY, zeaburQuery } from "@/api/queries"

afterEach(() => setSystemTime())

test("an open page learns of an upgrade without a reload", async () => {
  let version = "0.1.1"
  const fetched = spyOn(globalThis, "fetch").mockImplementation((async () =>
    Response.json({ version, updateCheck: true })) as unknown as typeof fetch)
  try {
    const client = new QueryClient()
    expect((await client.fetchQuery(zeaburQuery)).version).toBe("0.1.1")
    version = "0.2.0"
    expect((await client.fetchQuery(zeaburQuery)).version).toBe("0.1.1")
    setSystemTime(new Date(Date.now() + ZEABUR_EVERY + 1))
    expect((await client.fetchQuery(zeaburQuery)).version).toBe("0.2.0")
    expect(zeaburQuery.refetchInterval).toBe(ZEABUR_EVERY)
  } finally {
    fetched.mockRestore()
  }
})
