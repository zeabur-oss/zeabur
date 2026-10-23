import { describe, expect, test } from "bun:test"

import type { Schemas } from "@/api/client"
import {
  canIssue,
  certificateNow,
  issuerForm,
  issuerProblems,
  issuerRequest,
  pemProblems,
  solverLabel,
} from "@/model/cluster-issuers"

const route = (patch: Partial<Schemas["Route"]> = {}): Schemas["Route"] =>
  ({
    name: "shop",
    hostnames: ["shop.example.test"],
    backends: ["web"],
    origin: { kind: "Ingress", name: "shop" },
    ...patch,
  }) as Schemas["Route"]

describe("issuer form", () => {
  test("a new issuer needs its secret; a changed one keeps it when blank", () => {
    const form = issuerForm()
    expect(issuerProblems(form, false)).toEqual(["secret"])
    expect(issuerProblems(form, true)).toEqual([])
  })

  test("a provider's plain fields are required, an optional one is not", () => {
    const form = { ...issuerForm(), provider: "route53" as const, values: { region: "us-east-1" } }
    expect(issuerProblems(form, true)).toEqual(["field"])
    const filled = { ...form, values: { region: "us-east-1", accessKeyId: "AKIA" } }
    expect(issuerProblems(filled, true)).toEqual([])
  })

  test("the request sends only the chosen provider's fields", () => {
    const form = {
      ...issuerForm(),
      values: { apiToken: " token ", region: "left over" },
    }
    expect(issuerRequest(form).dns).toEqual({ provider: "cloudflare", apiToken: "token" })
  })

  test("an issuer the form can change reads back with its settings", () => {
    const form = issuerForm({
      name: "letsencrypt-dns",
      solvers: ["dns01:route53"],
      staging: true,
      wildcards: true,
      ready: "ready",
      dns: { provider: "route53", region: "eu-west-1", accessKeyId: "AKIA", secretAccessKey: "" },
    })
    expect(form.provider).toBe("route53")
    expect(form.staging).toBe(true)
    expect(form.values.region).toBe("eu-west-1")
  })
})

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
