import { describe, expect, test } from "bun:test"

import type { Schemas } from "@/api/client"
import type { BrowserCheck } from "@/model/browser-check"
import type { Overview } from "@/model/overview"
import type { Status } from "@/model/service"
import { verdictOf } from "@/model/verdict"

type Route = Schemas["RouteTraceView"]
type Diagnosis = Schemas["Diagnosis"]

const ready: Status = { health: "ok", word: "ready", ready: 2, desired: 2 }

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

type DomainCheck = Schemas["DomainCheck"]

const withDomain = (r: Route, domain: DomainCheck): Route => ({ ...r, domain })

const issuing: DomainCheck = {
  dns: "pointsHere",
  target: ["203.0.113.10"],
  certificate: { state: "issuing", name: "shop-tls" },
}

const diagnosis = (kind: Diagnosis["kind"]): Diagnosis => ({
  kind,
  cause: { key: `diagnosis.${kind}.cause.x` },
  why: { key: "w" },
  fix: { key: "f" },
  evidence: [],
  pods: [],
})

const said = (more: Partial<Overview> = {}): Pick<Overview, "causes" | "rollout" | "waiting"> => ({
  causes: [],
  rollout: { as: "none" },
  waiting: new Map(),
  ...more,
})

const none = () => undefined
const browser = (state: "reachable" | "unreachable") => (): BrowserCheck => ({ state, at: 0 })

describe("verdictOf", () => {
  test("a ready service whose domain the cluster reaches is running and reachable", () => {
    expect(
      verdictOf({
        status: ready,
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
      verdictOf({ status: ready, said: said(), routes, checks: browser("reachable") }).reach,
    ).toEqual({ as: "reachable", browser: true, domains: ["shop.example.com"] })
    expect(
      verdictOf({ status: ready, said: said(), routes, checks: browser("unreachable") }).reach,
    ).toEqual({ as: "unreachable", browser: true, domains: ["shop.example.com"] })
    expect(verdictOf({ status: ready, said: said(), routes, checks: none }).reach).toEqual({
      as: "unknown",
      browser: false,
      domains: ["shop.example.com"],
    })
  })

  test("a crashing service is a problem named by its reason, before anything else", () => {
    const verdict = verdictOf({
      status: { health: "fail", word: "failing", reason: "CrashLoopBackOff" },
      said: said({ causes: [diagnosis("crashLoop")], rollout: { as: "line", stuck: true } }),
      routes: [],
      checks: none,
    })
    expect(verdict).toEqual({
      tone: "fail",
      headline: { as: "problem", trouble: { as: "reason", word: "crashing" } },
      reach: undefined,
    })
  })

  test("without a kubelet reason the first cause's kind names the problem", () => {
    const verdict = verdictOf({
      status: { health: "warn", word: "degraded" },
      said: said({ causes: [diagnosis("probeFailure")] }),
      routes: [],
      checks: none,
    })
    expect(verdict.tone).toBe("warn")
    expect(verdict.headline).toEqual({ as: "problem", trouble: { as: "healthCheck" } })
  })

  test("a rollout under way is deploying", () => {
    expect(
      verdictOf({
        status: { health: "progress", word: "rollingOut" },
        said: said({ rollout: { as: "card" } }),
        routes: [],
        checks: none,
      }).headline,
    ).toEqual({ as: "deploying" })
  })

  test("a service scaled to zero is stopped, and its domains are not judged", () => {
    expect(
      verdictOf({
        status: { health: "idle", word: "scaledToZero", ready: 0, desired: 0 },
        said: said(),
        routes: [route("shop.example.com", "failed")],
        checks: none,
      }),
    ).toEqual({ tone: "idle", headline: { as: "stopped" } })
  })

  test("a running service with a domain that fails is a problem naming the domain", () => {
    const verdict = verdictOf({
      status: ready,
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
      status: { health: "fail", word: "failing", reason: "ImagePullBackOff" },
      said: said({
        causes: [diagnosis("imagePull")],
        waiting: new Map([["shop.example.com", "cause"]]),
      }),
      routes: [route("shop.example.com", "failed")],
      checks: none,
    })
    expect(verdict.headline).toEqual({
      as: "problem",
      trouble: { as: "reason", word: "imagePull" },
    })
    expect(verdict.reach).toBeUndefined()
  })

  test("a task keeps its own word unless it fails", () => {
    expect(
      verdictOf({
        status: { health: "idle", word: "scheduled", task: true },
        said: said(),
        routes: [],
        checks: none,
      }).headline,
    ).toEqual({ as: "status", word: "scheduled" })
  })

  test("a certificate still issuing is waited for, not a domain the browser can't reach", () => {
    const routes = [withDomain(route("shop.example.com", "unknown"), issuing)]
    const verdict = verdictOf({
      status: ready,
      said: said(),
      routes,
      checks: browser("unreachable"),
    })
    expect(verdict.headline).toEqual({ as: "running" })
    expect(verdict.reach).toEqual({ as: "issuing", browser: false, domains: ["shop.example.com"] })
  })

  test("a domain without its DNS record says so, as Networking does, with the record to add", () => {
    const missing: DomainCheck = { dns: "missing", target: ["203.0.113.10"] }
    const routes = [
      withDomain(route("shop.example.com", "unknown"), missing),
      route("www.example.com", "ok"),
    ]
    expect(verdictOf({ status: ready, said: said(), routes, checks: none }).reach).toEqual({
      as: "invalid",
      browser: false,
      domains: ["shop.example.com"],
      record: true,
    })
  })

  test("a domain the cluster fails to reach names its setup when that is the trouble", () => {
    const missing: DomainCheck = { dns: "missing", target: [] }
    const verdict = verdictOf({
      status: ready,
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
    }
    const routes = [withDomain(route("shop.example.com", "unknown"), elsewhere)]
    expect(verdictOf({ status: ready, said: said(), routes, checks: none }).reach).toEqual({
      as: "elsewhere",
      browser: false,
      domains: ["shop.example.com"],
      record: true,
    })
    expect(
      verdictOf({ status: ready, said: said(), routes, checks: browser("reachable") }).reach,
    ).toEqual({ as: "reachable", browser: true, domains: ["shop.example.com"] })
  })

  test("the cluster reaching a domain outweighs what its setup says", () => {
    const routes = [withDomain(route("shop.example.com", "ok"), issuing)]
    expect(verdictOf({ status: ready, said: said(), routes, checks: none }).reach).toEqual({
      as: "reachable",
      browser: false,
      domains: ["shop.example.com"],
    })
  })
})
