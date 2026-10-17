import { describe, test, expect } from "bun:test"

import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import {
  createMemoryHistory,
  createRootRoute,
  createRoute,
  createRouter,
  RouterProvider,
} from "@tanstack/react-router"
import { createElement } from "react"
import { renderToStaticMarkup } from "react-dom/server"

import { ApiError, type Problem } from "@/api/client"
import { capabilitiesQuery } from "@/api/queries"
import { ComponentsPage } from "@/routes/cluster/components"
// The page's search and navigation are typed by the app's router (its `Register`), which this
// test does not run.
import type {} from "@/router"

const refusal: Problem = {
  type: "/problems/forbidden",
  title: "You do not have permission",
  status: 403,
  detail: "The cluster does not let you read or change the cluster's components.",
  fix: "Ask a cluster administrator for access, or use an account that has it.",
  kind: "forbidden",
  summary: { key: "problem.summary.forbidden", params: { verb: "list", resource: "secrets" } },
  kubernetes: {
    code: 403,
    reason: "Forbidden",
    message: 'secrets is forbidden: cannot list "secrets"',
  },
  denied: { verb: "list", group: "", resource: "secrets" },
}

/** The Components page as it renders once `/cluster/capabilities` has answered with `error`. */
async function page(error: ApiError): Promise<string> {
  // The answer stays as it is: nothing is fetched again while rendering.
  const client = new QueryClient({ defaultOptions: { queries: { retryOnMount: false } } })
  await client.prefetchQuery({
    ...capabilitiesQuery,
    queryFn: () => Promise.reject(error),
    retry: false,
  })
  const rootRoute = createRootRoute()
  const components = createRoute({
    getParentRoute: () => rootRoute,
    path: "/cluster/components",
    component: ComponentsPage,
  })
  const router = createRouter({
    routeTree: rootRoute.addChildren([components]),
    history: createMemoryHistory({ initialEntries: ["/cluster/components"] }),
  })
  await router.load()
  return renderToStaticMarkup(
    createElement(QueryClientProvider, { client }, createElement(RouterProvider, { router })),
  )
}

describe("Components", () => {
  test("refused by the cluster are a quiet refusal with what to ask for, not an error or a catalogue", async () => {
    const markup = await page(new ApiError(403, refusal))
    expect(markup).toMatch(/the installed components/)
    expect(markup).not.toMatch(/role="alert"/)
    // No component is claimed missing or offered for install when none could be read.
    expect(markup).not.toMatch(/Not installed|Install/)
  })

  test("that fail otherwise are an error", async () => {
    const markup = await page(
      new ApiError(500, {
        ...refusal,
        kind: "internal",
        status: 500,
        kubernetes: undefined,
        denied: undefined,
      }),
    )
    expect(markup).toMatch(/role="alert"/)
    expect(markup).not.toMatch(/the installed components/)
  })
})
