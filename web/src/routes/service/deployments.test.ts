import { describe, test, expect } from "bun:test"

import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { createElement } from "react"
import { renderToStaticMarkup } from "react-dom/server"

import { releasesQuery, serviceTimelineQuery } from "@/api/queries"
import { changesOf, foldChanges, RECENT, weekOf } from "@/model/changes"
import type { Event, Service } from "@/model/service"
import { DeploymentsTab } from "@/routes/service/deployments"

function emptyTab(kind: string, name: string): string {
  const client = new QueryClient()
  client.setQueryData(releasesQuery("shop", kind, name).queryKey, { items: [] })
  client.setQueryData(serviceTimelineQuery("shop", kind, name).queryKey, {
    id: name,
    items: [],
    truncated: false,
    unreadable: [],
  } as never)
  const service = { kind, name, provenance: {} } as unknown as Service
  return renderToStaticMarkup(
    createElement(
      QueryClientProvider,
      { client },
      createElement(DeploymentsTab, { project: "shop", service, events: [], onEvents: () => {} }),
    ),
  )
}

describe("An empty Deployments tab", () => {
  test("says where a Deployment's deployments will show", () => {
    const html = emptyTab("Deployment", "worker")
    expect(html).toMatch(/a new deployment shows up here\./)
    expect(html).not.toMatch(/Jobs and CronJobs/)
  })

  test("says Jobs and CronJobs keep no releases", () => {
    expect(emptyTab("CronJob", "nightly")).toMatch(/Jobs and CronJobs keep no releases\./)
  })
})

const release = (revision: number, created: string) =>
  ({
    name: `api-${revision}`,
    revision,
    created,
    current: false,
    images: [],
    pods: { ready: 0, total: 0 },
    template: {},
    templateYaml: "",
  }) as never

const event = (type: string, lastSeen: string, id: string) =>
  ({ id, type, lastSeen, reason: "BackOff", note: "", count: 1, origin: {} }) as unknown as Event

describe("A service's changes", () => {
  test("interleave releases and warnings, newest first, and leave out normal events", () => {
    const entries = changesOf(
      [release(1, "2026-10-01T00:00:00Z"), release(2, "2026-10-03T00:00:00Z")],
      [event("Warning", "2026-10-02T00:00:00Z", "w"), event("Normal", "2026-10-04T00:00:00Z", "n")],
    )
    expect(entries.map((e) => e.kind)).toStrictEqual(["release", "warning", "release"])
  })
})

const entry = (kind: string, at: string, revision?: number) =>
  ({
    at,
    kind,
    workload: { kind: "Deployment", name: "api" },
    message: { key: `timeline.${kind}` },
    release: revision ? { revision, name: `api-${revision}`, images: [], current: false } : null,
  }) as never

describe("A service's changes from the timeline", () => {
  test("join releases by revision, keep writes, and leave events to the timeline", () => {
    const entries = changesOf(
      [release(1, "2026-10-01T00:00:00Z"), release(2, "2026-10-03T00:00:00Z")],
      [event("Warning", "2026-10-02T00:00:00Z", "w")],
      [entry("write", "2026-10-04T00:00:00Z"), entry("release", "2026-10-03T00:00:00Z", 2)],
    )
    expect(entries.map((e) => e.kind)).toStrictEqual(["timeline", "release", "release"])
    expect(entries[1]?.kind === "release" && entries[1].entry).toBeTruthy()
  })
})

describe("Folding a long history", () => {
  const now = Date.parse("2026-10-22T00:00:00Z")
  const hoursAgo = (h: number) => new Date(now - h * 3600e3).toISOString()
  const releases = (n: number) =>
    Array.from({ length: n }, (_, i) => release(n - i, hoursAgo(i * 7)))

  test("none, one and a few stay whole", () => {
    for (const n of [0, 1, RECENT, RECENT + 3]) {
      const { recent, older } = foldChanges(changesOf(releases(n), []), now)
      expect([recent.length, older.length]).toStrictEqual([n, 0])
    }
  })

  test("many keep the newest and this week's warnings, the rest under Older", () => {
    const entries = changesOf(releases(200), [event("Warning", hoursAgo(100), "w")])
    const { recent, older } = foldChanges(entries, now)
    expect(recent.length).toBe(RECENT + 1)
    expect(recent.at(-1)?.kind).toBe("warning")
    expect(older.length).toBe(200 - RECENT)
  })

  test("a warning older than a week stays under Older", () => {
    const entries = changesOf(releases(200), [event("Warning", hoursAgo(24 * 30), "w")])
    expect(foldChanges(entries, now).recent.length).toBe(RECENT)
  })

  test("the week counts releases and warnings in the last seven days", () => {
    const entries = changesOf(releases(200), [event("Warning", hoursAgo(1), "w")])
    expect(weekOf(entries, now)).toStrictEqual({ releases: 25, warnings: 1 })
  })
})
