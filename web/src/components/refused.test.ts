import { describe, test, expect } from "bun:test"

import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { createElement, type ReactElement } from "react"
import { renderToStaticMarkup } from "react-dom/server"

import type { Problem as ProblemBody, Schemas } from "@/api/client"
import { sessionQuery } from "@/api/queries"
import { Problem } from "@/components/problem"
import {
  hintOffersElevation,
  offersElevation,
  Refused,
  refusedAmong,
  refusedElsewhere,
  RefusedHint,
  RefusedKinds,
} from "@/components/refused"
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

/** The markup `element` renders to, with the session's elevation state already known. */
function render(element: ReactElement, elevation?: Schemas["ElevationState"]): string {
  const client = new QueryClient()
  if (elevation) client.setQueryData(sessionQuery.queryKey, { elevation } as never)
  return renderToStaticMarkup(createElement(QueryClientProvider, { client }, element))
}

const unreadable = (resource: string): Schemas["UnreadableView"] => ({
  resource,
  kubernetes: { ...kubernetes, message: `${resource} is forbidden` },
})

describe("Admin rights", () => {
  test("are offered where the server says they would help, unless they are in use", () => {
    const required = problem("elevation-required", true)
    expect(offersElevation(required, "available")).toBe(true)
    // Before the session is known, the server's word is enough.
    expect(offersElevation(required, undefined)).toBe(true)
    expect(offersElevation(required, "active")).toBe(false)
  })

  test("are offered for the cluster's refusals and unreadable parts only while offered", () => {
    const refused = problem("forbidden", true)
    for (const refusal of [refused, "unreadable"] as const) {
      expect(offersElevation(refusal, "available")).toBe(true)
      expect(offersElevation(refusal, "active")).toBe(false)
      expect(offersElevation(refusal, "unavailable")).toBe(false)
      expect(offersElevation(refusal, undefined)).toBe(false)
    }
  })

  test("are never offered for engawa's own refusals or other problems", () => {
    // "Only from the engawa page", "a plan made by someone else": no answer from the cluster.
    expect(offersElevation(problem("forbidden", false), "available")).toBe(false)
    expect(offersElevation(problem("not-found", true), "available")).toBe(false)
    expect(offersElevation(problem("conflict", false), "available")).toBe(false)
  })
})

describe("A refusal", () => {
  test("is a quiet line, while every other problem is an alert", () => {
    for (const kind of ["forbidden", "elevation-required"] as const) {
      const html = render(createElement(Problem, { problem: problem(kind, true) }), "available")
      expect(html).toMatch(/data-refused/)
      expect(html).not.toMatch(/role="alert"/)
    }
    for (const body of [
      problem("unavailable", false),
      problem("internal", false),
      // engawa's own 403 is not a missing permission.
      problem("forbidden", false),
    ]) {
      const html = render(createElement(Problem, { problem: body }), "available")
      expect(html).toMatch(/role="alert"/)
      expect(html).not.toMatch(/data-refused/)
    }
  })

  test("offers one action when admin rights are offered, and none otherwise", () => {
    const offered = render(createElement(Refused, { refusal: "unreadable" }), "available")
    expect(offered.match(/<button/g)?.length).toBe(1)
    expect(offered).toMatch(/View as admin/)
    expect(offered).not.toMatch(/Ask a cluster administrator/)

    const retry = render(
      createElement(Refused, { refusal: problem("elevation-required", true), retry: () => {} }),
      "available",
    )
    expect(retry).toMatch(/Retry as admin/)
    expect(retry).not.toMatch(/View as admin/)

    for (const elevation of ["unavailable", "active"] as const) {
      const html = render(createElement(Refused, { refusal: "unreadable" }), elevation)
      expect(html).not.toMatch(/as admin/)
      expect(html).toMatch(/needs more rights\. Ask a cluster administrator\./)
    }
  })

  test("keeps its details folded until asked", () => {
    const body = { ...problem("forbidden", true), detail: "The cluster refused it." }
    const html = render(
      createElement(Refused, { refusal: body, what: "Opening a terminal" }, "a Role to apply"),
      "available",
    )
    expect(html).toMatch(/Opening a terminal needs admin rights\./)
    expect(html).toMatch(/Details/)
    for (const folded of ["nodes is forbidden", "The cluster refused it.", "a Role to apply"])
      expect(!html.includes(folded), folded).toBeTruthy()
  })

  test("of kinds says which, in place", () => {
    const html = render(
      createElement(RefusedKinds, {
        unreadable: [unreadable("secrets"), unreadable("configmaps")],
      }),
      "available",
    )
    expect(html).toMatch(/Seeing Secrets and ConfigMaps needs admin rights\./)
    expect(render(createElement(RefusedKinds, { unreadable: [] }))).toBe("")
  })
})

describe("A permission hint that says no", () => {
  test("offers admin rights only when it says denied and they are offered, not in use", () => {
    expect(hintOffersElevation("denied", "available")).toBe(true)
    for (const elevation of ["active", "unavailable", undefined] as const)
      expect(hintOffersElevation("denied", elevation), String(elevation)).toBe(false)
    // An unknown hint leaves the page open: the plan's dry run decides.
    for (const hint of ["allowed", "unknown", undefined] as const)
      expect(hintOffersElevation(hint, "available"), String(hint)).toBe(false)
  })

  test("is the Refused line with Use admin rights, or View as admin for a read", () => {
    const change = render(
      createElement(RefusedHint, {
        hint: "denied",
        what: "Changing this project's details",
        missing: ["patch namespaces"],
      }),
      "available",
    )
    expect(change).toMatch(/data-refused/)
    expect(change).toMatch(/Changing this project&#x27;s details needs admin rights\./)
    expect(change).toMatch(/Use admin rights/)
    expect(change).not.toMatch(/Retry as admin/)

    const read = render(
      createElement(RefusedHint, { hint: "denied", what: "Seeing Secret", read: true }),
      "available",
    )
    expect(read).toMatch(/View as admin/)
  })

  test("leaves the page its own line when no rights are offered or the hint allows it", () => {
    for (const elevation of ["unavailable", "active"] as const)
      expect(render(createElement(RefusedHint, { hint: "denied", what: "x" }), elevation)).toBe("")
    expect(render(createElement(RefusedHint, { hint: "allowed", what: "x" }), "available")).toBe("")
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

  test("says what happened, why and what to do, and never offers admin rights", () => {
    const html = render(createElement(Problem, { problem: quota }), "available")
    expect(html).toMatch(/role="alert"/)
    expect(html).toMatch(/The change to web would go over the project&#x27;s limit\./)
    expect(html).toMatch(/allows 4 of requests\.cpu, and 3500m is already used/)
    expect(html).toMatch(/What to do/)
    expect(html).not.toMatch(/as admin/)
  })

  test("folds its evidence and the raw answer", () => {
    const html = render(createElement(Problem, { problem: quota }), "available")
    expect(html).toMatch(/Evidence/)
    expect(html).toMatch(/Details for Kubernetes users/)
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
