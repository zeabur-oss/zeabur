import { expect, spyOn, test } from "bun:test"
import { QueryClient } from "@tanstack/react-query"

import { ApiError } from "@/api/client"
import { ASK_AGAIN, pageQuery } from "@/api/pages"

const answering = (status: number) =>
  spyOn(globalThis, "fetch").mockImplementation(
    (async () => new Response(null, { status })) as unknown as typeof fetch,
  )

const client = () => new QueryClient({ defaultOptions: { queries: { retry: false } } })

test("a refusal is no", async () => {
  const fetched = answering(403)
  try {
    expect(await client().query(pageQuery("members"))).toBe(false)
  } finally {
    fetched.mockRestore()
  }
})

test("an answer the server could not give is an error, asked again later", async () => {
  const fetched = answering(503)
  try {
    const c = client()
    const page = pageQuery("members")
    await expect(c.query(page)).rejects.toBeInstanceOf(ApiError)
    const every = page.refetchInterval
    if (typeof every !== "function") throw new Error("refetchInterval is not a function")
    const query = c.getQueryCache().find({ queryKey: page.queryKey })
    expect(every(query as never)).toBe(ASK_AGAIN)
  } finally {
    fetched.mockRestore()
  }
})
