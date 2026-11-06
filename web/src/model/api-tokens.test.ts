import { describe, expect, it } from "bun:test"

import { expired, kubeconfig, startNamespace, validName } from "@/model/api-tokens"

const issued = {
  name: "ci",
  namespace: "zeabur-api-tokens",
  token: "eyJhbGciOi",
  expiresAt: "2026-12-06T00:00:00Z",
  ca: "-----BEGIN CERTIFICATE-----\nMIIB\n-----END CERTIFICATE-----\n",
  recorded: true,
}

describe("kubeconfig", () => {
  it("leaves the server for the person and carries the token, the CA and the namespace", () => {
    const yaml = kubeconfig(issued, "shop")
    expect(yaml).toContain('server: ""')
    expect(yaml).toContain("token: eyJhbGciOi")
    expect(yaml).toContain("namespace: shop")
    const data = /certificate-authority-data: (\S+)/.exec(yaml)?.[1] ?? ""
    expect(atob(data)).toBe(issued.ca)
  })

  it("leaves the CA out when it could not be read", () => {
    expect(kubeconfig({ ...issued, ca: null })).not.toContain("certificate-authority-data")
  })

  it("starts in no namespace when none is given", () => {
    expect(kubeconfig(issued)).not.toContain("namespace:")
  })
})

describe("names", () => {
  it("takes what kube-apiserver takes as a ServiceAccount's name", () => {
    expect(validName("github-actions")).toBe(true)
    expect(validName("CI")).toBe(false)
    expect(validName("-ci")).toBe(false)
    expect(validName("a".repeat(64))).toBe(false)
  })
})

describe("expiry", () => {
  it("is past once its time has come, and unknown is not past", () => {
    const now = Date.parse("2026-11-06T00:00:00Z")
    expect(expired("2026-11-05T23:59:59Z", now)).toBe(true)
    expect(expired("2026-11-06T00:00:01Z", now)).toBe(false)
    expect(expired(undefined, now)).toBe(false)
  })
})

describe("start namespace", () => {
  it("is the one project a token has a role in, and none otherwise", () => {
    expect(startNamespace([{ project: "shop", role: "viewer" }])).toBe("shop")
    expect(
      startNamespace([
        { project: "shop", role: "viewer" },
        { project: "blog", role: "viewer" },
      ]),
    ).toBeUndefined()
    expect(startNamespace([])).toBeUndefined()
  })
})
