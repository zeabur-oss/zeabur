import { describe, expect, it } from "bun:test"

import type { Schemas } from "@/api/client"
import { clusterRefusal, expired, kubeconfig, roleFor, validName } from "@/model/api-tokens"

const issued = {
  name: "ci",
  namespace: "shop",
  token: "eyJhbGciOi",
  expiresAt: "2026-12-06T00:00:00Z",
  ca: "-----BEGIN CERTIFICATE-----\nMIIB\n-----END CERTIFICATE-----\n",
  recorded: true,
}

describe("kubeconfig", () => {
  it("leaves the server for the person and carries the token, the CA and the namespace", () => {
    const yaml = kubeconfig(issued)
    expect(yaml).toContain('server: ""')
    expect(yaml).toContain("token: eyJhbGciOi")
    expect(yaml).toContain("namespace: shop")
    const data = /certificate-authority-data: (\S+)/.exec(yaml)?.[1] ?? ""
    expect(atob(data)).toBe(issued.ca)
  })

  it("leaves the CA out when it could not be read", () => {
    expect(kubeconfig({ ...issued, ca: null })).not.toContain("certificate-authority-data")
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

const choice = (
  role: Schemas.ProjectRole,
  cluster: boolean,
  permission: Schemas.Permission,
  refusal?: Schemas.ChoiceRefusal,
): Schemas.RoleChoice => ({ role, cluster, permission, refusal })

describe("role choices", () => {
  const viewerOnly = [
    choice("developer", false, "denied", "notHeld"),
    choice("viewer", false, "allowed"),
    choice("developer", true, "denied", "notAdmin"),
    choice("viewer", true, "denied", "notAdmin"),
  ]

  it("never lands on a role the person cannot give", () => {
    expect(roleFor(viewerOnly, false, "developer")).toBe("viewer")
    expect(roleFor(viewerOnly, false, undefined)).toBe("viewer")
    expect(roleFor(viewerOnly, true, "viewer")).toBeUndefined()
  })

  it("keeps the role picked while it is open", () => {
    const admin = viewerOnly.map((c) => ({ ...c, permission: "allowed" as const, refusal: null }))
    expect(roleFor(admin, true, "viewer")).toBe("viewer")
  })

  it("says why every namespace is closed, and nothing when it is open", () => {
    expect(clusterRefusal(viewerOnly)).toBe("notAdmin")
    expect(clusterRefusal([choice("viewer", true, "unknown")])).toBeUndefined()
  })
})
