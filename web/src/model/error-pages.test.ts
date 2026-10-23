import { describe, expect, test } from "bun:test"
import { pageChanges, pagesOf, type Pages } from "@/model/error-pages"

const route = { kind: "Ingress", name: "web" } as const
const defaults: Pages = {
  maintenance: "m",
  badGateway: "502",
  serviceUnavailable: "503",
  gatewayTimeout: "504",
}
const off = { maintenance: false, errorPages: false }

describe("pagesOf", () => {
  test("needs all four pages", () => {
    expect(pagesOf({ "maintenance.html": "m" })).toBeUndefined()
    expect(
      pagesOf({ "maintenance.html": "m", "502.html": "a", "503.html": "b", "504.html": "c" }),
    ).toEqual({ maintenance: "m", badGateway: "a", serviceUnavailable: "b", gatewayTimeout: "c" })
  })
})

describe("pageChanges", () => {
  test("first maintenance writes every page before the switch", () => {
    const changes = pageChanges(
      route,
      "www.example.com",
      { ...off, defaults },
      { maintenance: true, errorPages: false, edited: { badGateway: "mine" } },
    )
    expect(changes).toEqual([
      { op: "errorPageHtml", ...defaults, badGateway: "mine" },
      { op: "maintenance", route, hostname: "www.example.com", enabled: true },
    ])
  })

  test("later edits send only the changed pages", () => {
    const changes = pageChanges(
      route,
      "www.example.com",
      { ...off, pages: defaults, defaults },
      { ...off, edited: { maintenance: "m", gatewayTimeout: "new" } },
    )
    expect(changes).toEqual([{ op: "errorPageHtml", gatewayTimeout: "new" }])
  })

  test("turning off writes no pages", () => {
    const changes = pageChanges(
      route,
      "www.example.com",
      { maintenance: false, errorPages: true, defaults },
      { ...off, edited: {} },
    )
    expect(changes).toEqual([{ op: "errorPages", route, enabled: false }])
  })
})
