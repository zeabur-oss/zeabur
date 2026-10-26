import { describe, expect, test } from "bun:test"

import { servicePlace, databaseSearch, serviceSearch } from "@/routes/search"

describe("service places", () => {
  test("a place names its tab, and a part of Settings its anchor", () => {
    expect(servicePlace("variables")).toEqual({ tab: "variables" })
    expect(servicePlace("compute")).toEqual({ tab: "settings", hash: "compute" })
    expect(servicePlace("usage")).toEqual({ tab: "usage" })
    expect(servicePlace("events")).toEqual({ tab: "deployments", mode: "events" })
    expect(servicePlace("yaml")).toEqual({ tab: "settings", mode: "yaml" })
    expect(servicePlace("overview")).toEqual({})
  })

  test("a tab and its own view are kept; a view of another tab or an unknown tab is dropped", () => {
    expect(serviceSearch.parse({ tab: "usage" })).toEqual({ tab: "usage" })
    expect(serviceSearch.parse({ tab: "deployments", mode: "events" })).toMatchObject({
      tab: "deployments",
      mode: "events",
    })
    expect(serviceSearch.parse({ tab: "settings", mode: "events" })).toMatchObject({
      tab: "settings",
      mode: undefined,
    })
    expect(serviceSearch.parse({ tab: "nonsense" })).toMatchObject({ tab: undefined })
  })
})

describe("database places", () => {
  test("a tab, Settings' YAML view, and a pod where logs and events take one", () => {
    expect(databaseSearch.parse({ tab: "overview" })).toEqual({})
    expect(databaseSearch.parse({ tab: "settings", mode: "yaml" })).toEqual({
      tab: "settings",
      mode: "yaml",
    })
    expect(databaseSearch.parse({ tab: "backups", mode: "yaml", pod: "pg-1" })).toEqual({
      tab: "backups",
    })
    expect(databaseSearch.parse({ tab: "logs", pod: "pg-1" })).toEqual({
      tab: "logs",
      pod: "pg-1",
    })
    expect(databaseSearch.parse({ tab: "deployments" })).toEqual({})
  })
})
