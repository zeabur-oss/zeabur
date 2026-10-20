import { describe, expect, test } from "bun:test"

import { formerPlace, servicePlace } from "@/routes/search"

describe("service places", () => {
  test("the tabs from before the four sections land in their section", () => {
    expect(servicePlace("variables")).toEqual({ tab: "configure", hash: "variables" })
    expect(servicePlace("settings")).toEqual({ tab: "configure", hash: "compute" })
    expect(servicePlace("releases")).toEqual({ tab: "changes" })
    expect(servicePlace("logs")).toEqual({ tab: "inspect", view: "logs" })
    expect(servicePlace("metrics")).toEqual({ tab: "inspect", view: "usage" })
    expect(servicePlace("overview")).toEqual({})
  })

  test("a section is not a former place to redirect from", () => {
    expect(formerPlace("configure")).toBeUndefined()
    expect(formerPlace("networking")).toBe("networking")
    expect(formerPlace("nonsense")).toBeUndefined()
  })
})
