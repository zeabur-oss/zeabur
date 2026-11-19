import { describe, expect, test } from "bun:test"

import type { Schemas } from "@/api/client"
import type { BrowserCheck } from "@/model/browser-check"
import type { Overview } from "@/model/overview"
import { verdictOf } from "@/model/verdict"

type Route = Schemas.RouteTraceView
type DomainCheck = Schemas.DomainCheck

const running: Schemas.ServiceVerdict = { tone: "ok", headline: { as: "running" } }

const route = (
  address: string,
  status: Route["status"],
  kind: Route["kind"] = "domain",
): Route => ({
  kind,
  address,
  status,
  hops: [],
  via: { kind: "Ingress", name: "web" },
})

const withDomain = (r: Route, domain: DomainCheck): Route => ({ ...r, domain })

const issuing: DomainCheck = {
  dns: "pointsHere",
  target: ["203.0.113.10"],
  certificate: { state: "issuing", name: "shop-tls" },
  setup: "pending",
}

const said = (more: Partial<Overview> = {}): Pick<Overview, "rollout" | "waiting"> => ({
  rollout: { as: "none" },
  waiting: new Map(),
  ...more,
})

const none = () => undefined
const browser = (state: "reachable" | "unreachable") => (): BrowserCheck => ({ state, at: 0 })

describe("verdictOf", () => {
  test("a running service whose domain the cluster reaches is running and reachable", () => {
    expect(
      verdictOf({
        verdict: running,
        said: said(),
        routes: [
          route("shop.example.com", "ok"),
          route("web.shop.svc.cluster.local", "ok", "private"),
        ],
        checks: none,
      }),
    ).toEqual({
      tone: "ok",
      headline: { as: "running" },
      reach: { as: "reachable", browser: false, domains: ["shop.example.com"] },
    })
  })

  test("a domain the cluster cannot check takes the browser's answer", () => {
    const routes = [route("shop.example.com", "unknown")]
    expect(
      verdictOf({ verdict: running, said: said(), routes, checks: browser("reachable") }).reach,
    ).toEqual({ as: "reachable", browser: true, domains: ["shop.example.com"] })
    expect(
      verdictOf({ verdict: running, said: said(), routes, checks: browser("unreachable") }).reach,
    ).toEqual({ as: "unreachable", browser: true, domains: ["shop.example.com"] })
    expect(verdictOf({ verdict: running, said: said(), routes, checks: none }).reach).toEqual({
      as: "unknown",
      browser: false,
      domains: ["shop.example.com"],
    })
  })

  test("the server's problem stays the headline", () => {
    const verdict = verdictOf({
      verdict: {
        tone: "fail",
        headline: { as: "problem", trouble: { as: "reason", word: "crashing" } },
      },
      said: said({ rollout: { as: "card" } }),
      routes: [route("shop.example.com", "failed")],
      checks: none,
    })
    expect(verdict.tone).toBe("fail")
    expect(verdict.headline).toEqual({
      as: "problem",
      trouble: { as: "reason", word: "crashing" },
    })
  })

  test("a running service with a rollout card is deploying", () => {
    const verdict = verdictOf({
      verdict: running,
      said: said({ rollout: { as: "card" } }),
      routes: [],
      checks: none,
    })
    expect(verdict.tone).toBe("progress")
    expect(verdict.headline).toEqual({ as: "deploying" })
  })

  test("a stopped service's domains are not judged", () => {
    expect(
      verdictOf({
        verdict: { tone: "idle", headline: { as: "stopped" } },
        said: said(),
        routes: [route("shop.example.com", "failed")],
        checks: none,
      }),
    ).toEqual({ tone: "idle", headline: { as: "stopped" } })
  })

  test("a running service with a domain that fails is a problem naming the domain", () => {
    const verdict = verdictOf({
      verdict: running,
      said: said(),
      routes: [route("shop.example.com", "failed"), route("api.example.com", "ok")],
      checks: none,
    })
    expect(verdict.tone).toBe("fail")
    expect(verdict.headline).toEqual({
      as: "problem",
      trouble: { as: "unreachable", domain: "shop.example.com" },
    })
    expect(verdict.reach).toBeUndefined()
  })

  test("domains that only wait on the cause shown are left out of the reach line", () => {
    const verdict = verdictOf({
      verdict: {
        tone: "fail",
        headline: { as: "problem", trouble: { as: "reason", word: "imagePull" } },
      },
      said: said({ waiting: new Map([["shop.example.com", "cause"]]) }),
      routes: [route("shop.example.com", "failed")],
      checks: none,
    })
    expect(verdict.reach).toBeUndefined()
  })

  test("a certificate still issuing is waited for, not a domain the browser can't reach", () => {
    const routes = [withDomain(route("shop.example.com", "unknown"), issuing)]
    const verdict = verdictOf({
      verdict: running,
      said: said(),
      routes,
      checks: browser("unreachable"),
    })
    expect(verdict.headline).toEqual({ as: "running" })
    expect(verdict.reach).toEqual({ as: "issuing", browser: false, domains: ["shop.example.com"] })
  })

  test("a domain without its DNS record says so, as Networking does, with the record to add", () => {
    const missing: DomainCheck = { dns: "missing", target: ["203.0.113.10"], setup: "invalid" }
    const routes = [
      withDomain(route("shop.example.com", "unknown"), missing),
      route("www.example.com", "ok"),
    ]
    expect(verdictOf({ verdict: running, said: said(), routes, checks: none }).reach).toEqual({
      as: "invalid",
      browser: false,
      domains: ["shop.example.com"],
      record: true,
    })
  })

  test("a domain the cluster fails to reach names its setup when that is the trouble", () => {
    const missing: DomainCheck = { dns: "missing", target: [], setup: "invalid" }
    const verdict = verdictOf({
      verdict: running,
      said: said(),
      routes: [withDomain(route("shop.example.com", "failed"), missing)],
      checks: none,
    })
    expect(verdict.headline).toEqual({
      as: "problem",
      trouble: { as: "unreachable", domain: "shop.example.com" },
    })
    expect(verdict.reach).toEqual({ as: "invalid", browser: false, domains: ["shop.example.com"] })
  })

  test("a domain pointing elsewhere is said so unless the browser reaches it", () => {
    const elsewhere: DomainCheck = {
      dns: "elsewhere",
      target: ["203.0.113.10"],
      resolved: ["198.51.100.7"],
      setup: "elsewhere",
    }
    const routes = [withDomain(route("shop.example.com", "unknown"), elsewhere)]
    expect(verdictOf({ verdict: running, said: said(), routes, checks: none }).reach).toEqual({
      as: "elsewhere",
      browser: false,
      domains: ["shop.example.com"],
      record: true,
    })
    expect(
      verdictOf({ verdict: running, said: said(), routes, checks: browser("reachable") }).reach,
    ).toEqual({ as: "reachable", browser: true, domains: ["shop.example.com"] })
  })

  test("the cluster reaching a domain outweighs what its setup says", () => {
    const routes = [withDomain(route("shop.example.com", "ok"), issuing)]
    expect(verdictOf({ verdict: running, said: said(), routes, checks: none }).reach).toEqual({
      as: "reachable",
      browser: false,
      domains: ["shop.example.com"],
    })
  })
})
