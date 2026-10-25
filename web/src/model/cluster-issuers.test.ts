import { describe, expect, test } from "bun:test"

import type { Schemas } from "@/api/client"
import { canIssue, certificateNow, pemProblems, solverLabel } from "@/model/cluster-issuers"

const route = (patch: Partial<Schemas["Route"]> = {}): Schemas["Route"] =>
  ({
    name: "shop",
    hostnames: ["shop.example.test"],
    backends: ["web"],
    origin: { kind: "Ingress", name: "shop" },
    ...patch,
  }) as Schemas["Route"]

test("solvers read as people write them", () => {
  expect(solverLabel("http01")).toBe("HTTP-01")
  expect(solverLabel("dns01:cloudflare")).toBe("DNS-01 (cloudflare)")
})

describe("certificates of a domain", () => {
  test("a wildcard needs an issuer that can sign one", () => {
    expect(canIssue({ wildcards: false }, "*.example.test")).toBe(false)
    expect(canIssue({ wildcards: false }, "shop.example.test")).toBe(true)
    expect(canIssue({ wildcards: true }, "*.example.test")).toBe(true)
  })

  test("where the certificate comes from now", () => {
    const tls = [{ host: "shop.example.test", secret: "shop-tls" }]
    expect(certificateNow(route({ issuer: "letsencrypt", tls }), "shop.example.test")).toEqual({
      as: "issuer",
      issuer: "letsencrypt",
      secret: "shop-tls",
    })
    expect(certificateNow(route({ tls }), "shop.example.test")).toEqual({
      as: "secret",
      secret: "shop-tls",
    })
    expect(certificateNow(route(), "shop.example.test")).toEqual({ as: "none" })
  })

  test("pasted PEM is looked at before the plan", () => {
    const cert = "-----BEGIN CERTIFICATE-----\nMII\n-----END CERTIFICATE-----"
    const key = "-----BEGIN PRIVATE KEY-----\nMII\n-----END PRIVATE KEY-----"
    expect(pemProblems(cert, key)).toEqual([])
    expect(pemProblems(key, cert)).toEqual(["certificate", "key"])
  })
})
