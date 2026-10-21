import { describe, expect, test } from "bun:test"

import { formerPlace, servicePlace, validateServiceSearch } from "@/routes/search"

describe("service places", () => {
  test("a place names its tab, and a part of Settings its anchor", () => {
    expect(servicePlace("variables")).toEqual({ tab: "variables" })
    expect(servicePlace("compute")).toEqual({ tab: "settings", hash: "compute" })
    expect(servicePlace("releases")).toEqual({ tab: "deployments" })
    expect(servicePlace("metrics")).toEqual({ tab: "usage" })
    expect(servicePlace("overview")).toEqual({})
  })

  test("links from the four sections lead to the tab that holds them now", () => {
    expect(formerPlace("status")).toEqual({})
    expect(formerPlace("changes")).toEqual({ tab: "deployments" })
    expect(formerPlace("inspect", "terminal")).toEqual({ tab: "terminal" })
    expect(formerPlace("inspect", "metrics")).toEqual({ tab: "usage" })
    expect(formerPlace("inspect")).toEqual({ tab: "logs" })
    expect(formerPlace("configure", undefined, "networking")).toEqual({ tab: "networking" })
    expect(formerPlace("configure", undefined, "health-checks")).toEqual({
      tab: "settings",
      hash: "health-checks",
    })
    expect(formerPlace("configure")).toEqual({ tab: "variables" })
  })

  test("a former tab is kept for the redirect; an unknown one is dropped", () => {
    expect(validateServiceSearch({ tab: "inspect", view: "yaml" })).toMatchObject({
      tab: undefined,
      former: "inspect",
      view: "yaml",
    })
    expect(validateServiceSearch({ tab: "logs" })).toMatchObject({ tab: "logs", former: undefined })
    expect(validateServiceSearch({ tab: "nonsense" })).toMatchObject({
      tab: undefined,
      former: undefined,
    })
  })
})
