import { describe, expect, test } from "bun:test"
import { domainsLead, filterRoutes, plainFirst } from "@/model/domains"
import type { PublicRoute } from "@/model/service"

const route = (host: string | undefined, https: boolean, path?: string): PublicRoute => ({
  host,
  path,
  ingress: "web",
  backend: "web",
  https,
})

describe("domainsLead", () => {
  test("no routes", () => {
    expect(domainsLead([])).toEqual({ hosts: 0, plain: 0 })
  })
  test("one host", () => {
    expect(domainsLead([route("a.example.com", true)])).toEqual({ hosts: 1, plain: 0 })
  })
  test("counts a host once across paths, plain if any rule lacks TLS", () => {
    expect(
      domainsLead([
        route("a.example.com", true, "/"),
        route("a.example.com", false, "/api"),
        route("b.example.com", true),
        route(undefined, false),
      ]),
    ).toEqual({ hosts: 2, plain: 1 })
  })
  test("many hosts", () => {
    const routes = Array.from({ length: 15 }, (_, i) => route(`h${i}.example.com`, i % 4 !== 0))
    expect(domainsLead(routes)).toEqual({ hosts: 15, plain: 4 })
  })
})

describe("plainFirst", () => {
  test("empty and single lists stay as they are", () => {
    expect(plainFirst([])).toEqual([])
    const one = [route("a.example.com", true)]
    expect(plainFirst(one)).toEqual(one)
  })
  test("moves hosts without HTTPS up, keeping order otherwise", () => {
    const routes = [
      route("a.example.com", true),
      route("b.example.com", false),
      route(undefined, false),
      route("c.example.com", false),
    ]
    expect(plainFirst(routes).map((r) => r.host)).toEqual([
      "b.example.com",
      "c.example.com",
      "a.example.com",
      undefined,
    ])
  })
})

describe("filterRoutes", () => {
  const routes = [route("shop.example.com", true, "/"), route("api.example.com", true, "/v1")]
  test("an empty query keeps every route", () => {
    expect(filterRoutes(routes, " ")).toBe(routes)
  })
  test("matches host or path, ignoring case", () => {
    expect(filterRoutes(routes, "SHOP").map((r) => r.host)).toEqual(["shop.example.com"])
    expect(filterRoutes(routes, "/v1").map((r) => r.host)).toEqual(["api.example.com"])
    expect(filterRoutes(routes, "nothing")).toEqual([])
  })
})
