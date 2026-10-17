import { describe, test, expect } from "bun:test"

import { defaultClaimName, subdomain, volumeRequest, type VolumeValues } from "./volume"

const values = (overrides: Partial<VolumeValues> = {}): VolumeValues => ({
  name: "uploads",
  mountPath: "/srv/uploads",
  size: "5Gi",
  claimName: null,
  accessMode: "ReadWriteOnce",
  ...overrides,
})

describe("the new-volume form", () => {
  test("leaves the claim name and access mode to the server's defaults when untouched", () => {
    expect(volumeRequest("web", values())).toStrictEqual({
      name: "uploads",
      mountPath: "/srv/uploads",
      size: "5Gi",
      claimName: undefined,
      accessMode: undefined,
    })
  })

  test("shows the default claim name the server would choose", () => {
    expect(defaultClaimName("web", " uploads ")).toBe("web-uploads")
    expect(defaultClaimName("web", "")).toBe("")
  })

  test("sends a chosen claim name and access mode", () => {
    const request = volumeRequest(
      "web",
      values({ claimName: " shared-uploads ", accessMode: "ReadWriteMany" }),
    )
    expect(request.claimName).toBe("shared-uploads")
    expect(request.accessMode).toBe("ReadWriteMany")
  })

  test("does not send a claim name typed back to the default", () => {
    expect(volumeRequest("web", values({ claimName: "web-uploads" })).claimName).toBe(undefined)
  })

  test("accepts claim names Kubernetes accepts", () => {
    for (const name of ["data", "web-uploads", "web.uploads-2", "a".repeat(253)]) {
      expect(subdomain.test(name), name).toBeTruthy()
    }
    for (const name of ["", "Data", "web_uploads", "-web", "web.", "a..b", "a".repeat(254)]) {
      expect(!subdomain.test(name), name).toBeTruthy()
    }
  })
})
