import { describe, expect, test } from "bun:test"
import type { Schemas } from "@/api/client"
import {
  dnsSetup,
  domainsLead,
  filterRoutes,
  isSslip,
  plainFirst,
  recordsFor,
  dashedSuffix,
  sslipSuffix,
  wildcardFor,
} from "@/model/domains"
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

const check = (more: Partial<Schemas.DomainCheck> = {}): Schemas.DomainCheck => ({
  target: ["203.0.113.10"],
  dns: "missing",
  setup: "invalid",
  ...more,
})

describe("recordsFor", () => {
  test("an A record per IPv4 address and AAAA per IPv6", () => {
    expect(recordsFor("shop.example.com", ["203.0.113.10", "2001:db8::1"])).toEqual([
      { type: "A", name: "shop.example.com", value: "203.0.113.10" },
      { type: "AAAA", name: "shop.example.com", value: "2001:db8::1" },
    ])
  })
  test("one CNAME to a load balancer's host name", () => {
    expect(
      recordsFor("shop.example.com", ["lb-1.elb.example.net", "lb-2.elb.example.net"]),
    ).toEqual([{ type: "CNAME", name: "shop.example.com", value: "lb-1.elb.example.net" }])
  })
  test("nothing when no address is known", () => {
    expect(recordsFor("shop.example.com", [])).toEqual([])
  })
})

describe("wildcardFor", () => {
  test("one label under the suffix is covered by its wildcard", () => {
    expect(wildcardFor("web-shop.apps.example.com", ["example.com", "apps.example.com"])).toBe(
      "*.apps.example.com",
    )
  })
  test("deeper names, the suffix itself and other domains are not", () => {
    expect(wildcardFor("a.b.apps.example.com", ["apps.example.com"])).toBeUndefined()
    expect(wildcardFor("apps.example.com", ["apps.example.com"])).toBeUndefined()
    expect(wildcardFor("shop.example.com", ["apps.example.com"])).toBeUndefined()
    expect(wildcardFor("shop.example.com")).toBeUndefined()
  })
})

describe("sslip.io", () => {
  test("a suffix keeps the address in one label", () => {
    expect(sslipSuffix("203.0.113.7")).toBe("203-0-113-7.sslip.io")
    expect(sslipSuffix("2001:db8::1")).toBe("2001-db8--1.sslip.io")
    expect(isSslip("web-shop.203-0-113-7.sslip.io")).toBe(true)
    expect(isSslip("web.example.com")).toBe(false)
  })

  test("a dotted suffix is written with dashes, so a name that ends in a digit stays apart", () => {
    expect(dashedSuffix("172.238.20.4.sslip.io")).toBe("172-238-20-4.sslip.io")
    expect(dashedSuffix("10.0.0.1.nip.io")).toBe("10-0-0-1.nip.io")
    expect(dashedSuffix("203-0-113-7.sslip.io")).toBe("203-0-113-7.sslip.io")
    expect(dashedSuffix("apps.example.com")).toBe("apps.example.com")
  })

  test("a wrong sslip.io name is a problem with the name, not a record to add", () => {
    const wrong = check({ dns: "elsewhere", resolved: ["5.172.238.20"] })
    expect(dnsSetup("web-5.172.238.20.4.sslip.io", wrong)).toEqual({
      as: "name",
      resolved: ["5.172.238.20"],
    })
  })

  test("a dashed sslip.io name that moved is a record to add, as any other domain", () => {
    const moved = check({ dns: "elsewhere", resolved: ["203.0.113.7"] })
    expect(dnsSetup("web-shop.203-0-113-7.sslip.io", moved).as).toBe("elsewhere")
  })
})

describe("dnsSetup", () => {
  test("pointing at the cluster needs nothing", () => {
    expect(dnsSetup("shop.example.com", check({ dns: "pointsHere" }))).toEqual({ as: "valid" })
  })
  test("a missing record names the record to add", () => {
    expect(dnsSetup("shop.example.com", check())).toEqual({
      as: "missing",
      records: [{ type: "A", name: "shop.example.com", value: "203.0.113.10" }],
    })
  })
  test("a name under the suffix needs the wildcard, not its own record", () => {
    expect(dnsSetup("web.apps.example.com", check(), ["apps.example.com"])).toEqual({
      as: "missing",
      records: [{ type: "A", name: "*.apps.example.com", value: "203.0.113.10" }],
    })
  })
  test("another address is said with what it resolved to", () => {
    expect(
      dnsSetup("shop.example.com", check({ dns: "elsewhere", resolved: ["198.51.100.7"] })),
    ).toMatchObject({ as: "elsewhere", resolved: ["198.51.100.7"] })
  })
})
