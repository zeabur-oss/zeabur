import { describe, expect, test } from "bun:test"

import { formerPlace, servicePlace, validateServiceSearch } from "@/routes/search"

describe("service places", () => {
  test("a place names its tab, and a part of Settings its anchor", () => {
    expect(servicePlace("variables")).toEqual({ tab: "variables" })
    expect(servicePlace("compute")).toEqual({ tab: "settings", hash: "compute" })
    expect(servicePlace("releases")).toEqual({ tab: "deployments" })
    expect(servicePlace("metrics")).toEqual({ tab: "usage" })
    expect(servicePlace("usage")).toEqual({ tab: "usage" })
    expect(servicePlace("events")).toEqual({ tab: "deployments", mode: "events" })
    expect(servicePlace("yaml")).toEqual({ tab: "settings", mode: "yaml" })
    expect(servicePlace("overview")).toEqual({})
  })

  test("links from the four sections lead to the tab that holds them now", () => {
    expect(formerPlace("status")).toEqual({})
    expect(formerPlace("changes")).toEqual({ tab: "deployments" })
    expect(formerPlace("inspect", "terminal")).toEqual({ tab: "logs", terminal: true })
    expect(formerPlace("inspect", "metrics")).toEqual({ tab: "usage" })
    expect(formerPlace("inspect")).toEqual({ tab: "logs" })
    expect(formerPlace("configure", undefined, "networking")).toEqual({ tab: "networking" })
    expect(formerPlace("configure", undefined, "health-checks")).toEqual({
      tab: "settings",
      hash: "health-checks",
    })
    expect(formerPlace("configure")).toEqual({ tab: "variables" })
  })

  test("a view that was a tab of its own opens inside the tab that holds it", () => {
    expect(formerPlace("terminal")).toEqual({ tab: "logs", terminal: true })
    expect(formerPlace("events")).toEqual({ tab: "deployments", mode: "events" })
    expect(formerPlace("yaml")).toEqual({ tab: "settings", mode: "yaml" })
    expect(validateServiceSearch({ tab: "usage" })).toMatchObject({
      tab: "usage",
      former: undefined,
    })
  })

  test("a former tab is kept for the redirect; an unknown one is dropped", () => {
    expect(validateServiceSearch({ tab: "inspect", view: "yaml" })).toMatchObject({
      tab: undefined,
      former: "inspect",
      view: "yaml",
    })
    expect(validateServiceSearch({ tab: "logs" })).toMatchObject({ tab: "logs", former: undefined })
    expect(validateServiceSearch({ tab: "terminal" })).toMatchObject({
      tab: undefined,
      former: "terminal",
    })
    // Logs' terminal view is the dock now: the link opens Logs with the dock.
    expect(validateServiceSearch({ tab: "logs", mode: "terminal" })).toMatchObject({
      tab: "logs",
      mode: undefined,
      former: "terminal",
    })
    expect(validateServiceSearch({ tab: "settings", mode: "events" })).toMatchObject({
      tab: "settings",
      mode: undefined,
    })
    expect(validateServiceSearch({ tab: "nonsense" })).toMatchObject({
      tab: undefined,
      former: undefined,
    })
  })
})
