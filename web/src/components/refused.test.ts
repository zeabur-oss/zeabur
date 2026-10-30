import { describe, test, expect } from "bun:test"

import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { createElement, type ReactElement } from "react"
import { renderToStaticMarkup } from "react-dom/server"

import type { Problem as ProblemBody, Schemas } from "@/api/client"
import { Problem } from "@/components/problem"
import { Refused, refusedAmong, refusedElsewhere, RefusedKinds } from "@/components/refused"
import { inServiceTabs, refusedPlaces } from "@/routes/project/data"

const kubernetes = { code: 403, reason: "Forbidden", message: "nodes is forbidden" }

function problem(kind: ProblemBody["kind"], fromCluster: boolean): ProblemBody {
  return {
    type: `/problems/${kind}`,
    title: "",
    status: 403,
    detail: "",
    fix: "",
    kind,
    summary: { key: `problem.summary.${kind}` },
    ...(fromCluster ? { kubernetes } : {}),
  }
}

function render(element: ReactElement): string {
  const client = new QueryClient()
  return renderToStaticMarkup(createElement(QueryClientProvider, { client }, element))
}

const unreadable = (resource: string): Schemas.UnreadableView => ({
  resource,
  kubernetes: { ...kubernetes, message: `${resource} is forbidden` },
})

describe("A refusal", () => {
  test("is a quiet line, while every other problem is an alert", () => {
    const html = render(createElement(Problem, { problem: problem("forbidden", true) }))
    expect(html).toMatch(/needs more rights/)
    expect(html).not.toMatch(/role="alert"/)
    for (const body of [
      problem("unavailable", false),
      problem("internal", false),
      // Zeabur's own 403 is not a missing permission.
      problem("forbidden", false),
    ]) {
      const html = render(createElement(Problem, { problem: body }))
      expect(html).toMatch(/role="alert"/)
      expect(html).not.toMatch(/needs more rights/)
    }
  })

  test("says who to ask", () => {
    const html = render(createElement(Refused, { refusal: "unreadable" }))
    expect(html).toMatch(/needs more rights\. Ask a cluster administrator\./)
  })

  test("keeps its details folded until asked", () => {
    const body = { ...problem("forbidden", true), detail: "The cluster refused it." }
    const html = render(
      createElement(Refused, { refusal: body, what: "Opening a terminal" }, "a Role to apply"),
    )
    expect(html).toMatch(/Opening a terminal needs more rights\./)
    expect(html).toMatch(/Details/)
    for (const folded of ["nodes is forbidden", "The cluster refused it.", "a Role to apply"])
      expect(!html.includes(folded), folded).toBeTruthy()
  })

  test("of kinds says which, in place", () => {
    const html = render(
      createElement(RefusedKinds, {
        unreadable: [unreadable("secrets"), unreadable("configmaps")],
      }),
    )
    expect(html).toMatch(/Seeing Secrets and ConfigMaps needs more rights\./)
    expect(render(createElement(RefusedKinds, { unreadable: [] }))).toBe("")
  })
})

describe("A problem that is not a refusal", () => {
  const quota: ProblemBody = {
    ...problem("quota-exceeded", true),
    summary: { key: "problem.summary.quota-exceeded", params: { name: "web" } },
    why: {
      key: "problem.why.quota-exceeded",
      params: { quota: "compute", resource: "requests.cpu", hard: "4", used: "3500m" },
    },
    fixKey: { key: "problem.quotaExceeded.fix.scale", params: { quota: "compute" } },
    evidence: [
      {
        kind: "ResourceQuota",
        name: "compute",
        fieldPath: "status.used.requests.cpu",
        value: "3500m",
      },
    ],
  }

  test("says what happened, why and what to do", () => {
    const html = render(createElement(Problem, { problem: quota }))
    expect(html).toMatch(/role="alert"/)
    expect(html).toMatch(/The change to web would go over the project&#x27;s limit\./)
    expect(html).toMatch(/allows 4 of requests\.cpu, and 3500m is already used/)
    expect(html).toMatch(/What to do/)
  })

  test("folds its evidence and the raw answer", () => {
    const html = render(createElement(Problem, { problem: quota }))
    expect(html).toMatch(/Evidence/)
    expect(html).toMatch(/Raw error/)
    for (const folded of ["ResourceQuota/compute", "nodes is forbidden"])
      expect(!html.includes(folded), folded).toBeTruthy()
  })
})

describe("Unreadable kinds on the service page", () => {
  const kinds = Object.values(refusedPlaces).flat()

  test("each show in one place only", () => {
    expect(new Set(kinds).size).toBe(kinds.length)
  })

  test("show in their tab, and only the rest under the header", () => {
    const all = [...kinds, "leases.coordination.k8s.io"].map(unreadable)
    const variables = refusedAmong(all, refusedPlaces.config).map((u) => u.resource)
    expect(variables).toStrictEqual(["secrets", "configmaps"])
    const header = refusedElsewhere(all, inServiceTabs).map((u) => u.resource)
    expect(header).toStrictEqual([...refusedPlaces.services, "leases.coordination.k8s.io"])
    expect(!header.includes("secrets")).toBeTruthy()
    expect(!header.includes("httproutes.gateway.networking.k8s.io")).toBeTruthy()
  })
})
