import { describe, test, expect } from "bun:test"

import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { createElement } from "react"
import { renderToStaticMarkup } from "react-dom/server"

import { releasesQuery } from "@/api/queries"
import type { Service } from "@/model/service"
import { ReleasesTab } from "@/routes/service/releases"

/** The Releases tab of a `kind` named `name` once the server has answered with no releases. */
function emptyTab(kind: string, name: string): string {
  const client = new QueryClient()
  client.setQueryData(releasesQuery("shop", kind, name).queryKey, { items: [] })
  const service = { kind, name, provenance: {} } as unknown as Service
  return renderToStaticMarkup(
    createElement(
      QueryClientProvider,
      { client },
      createElement(ReleasesTab, { project: "shop", service }),
    ),
  )
}

describe("An empty Releases tab", () => {
  test("says a Deployment has no earlier release yet", () => {
    const html = emptyTab("Deployment", "worker")
    expect(html).toMatch(/No earlier releases yet\./)
    expect(html).not.toMatch(/Jobs and CronJobs/)
  })

  test("says Jobs and CronJobs keep no releases", () => {
    expect(emptyTab("CronJob", "nightly")).toMatch(/Jobs and CronJobs keep no releases\./)
  })
})
