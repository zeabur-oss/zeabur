import { describe, expect, it } from "bun:test"

import { anyOpen, clusterPages, pagesOfRoute } from "@/model/pages"

describe("pages", () => {
  it("shows a page when its answer is yes", () => {
    expect(anyOpen({ members: true }, ["members"])).toBe(true)
  })

  it("keeps a page closed when its answer is no, missing or failed", () => {
    expect(anyOpen({ members: false }, ["members"])).toBe(false)
    expect(anyOpen({}, ["members"])).toBe(false)
  })

  it("opens the Cluster area when one of its pages opens", () => {
    expect(anyOpen({ backups: true }, clusterPages)).toBe(true)
    expect(anyOpen({ members: true }, clusterPages)).toBe(false)
  })

  it("leaves a page that is not for administrators alone", () => {
    expect(pagesOfRoute("/projects")).toBeUndefined()
  })
})
