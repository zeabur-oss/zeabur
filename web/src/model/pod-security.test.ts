import { describe, expect, test } from "bun:test"

import { lowersProject, profileChanges, templateSecurity } from "@/model/pod-security"

describe("security profiles", () => {
  test("Strict writes the restricted settings and Compatible lets root back in", () => {
    const strict = templateSecurity("restricted")
    expect(strict.pod).toStrictEqual({
      automountServiceAccountToken: false,
      securityContext: { runAsNonRoot: true, seccompProfile: { type: "RuntimeDefault" } },
    })
    expect(strict.container).toStrictEqual({
      securityContext: { allowPrivilegeEscalation: false, capabilities: { drop: ["ALL"] } },
    })
    const compatible = templateSecurity("baseline")
    expect(compatible.pod.securityContext).toStrictEqual({
      seccompProfile: { type: "RuntimeDefault" },
    })
    expect(compatible.container).toStrictEqual({
      securityContext: { allowPrivilegeEscalation: false },
    })
  })

  test("Compatible lowers only a project that enforces restricted", () => {
    expect(lowersProject("baseline", "restricted")).toBe(true)
    expect(lowersProject("baseline", "baseline")).toBe(false)
    expect(lowersProject("baseline", undefined)).toBe(false)
    expect(lowersProject("restricted", "restricted")).toBe(false)
    const web = { kind: "Deployment" as const, name: "web" }
    expect(profileChanges(web, "baseline", "restricted")).toStrictEqual([
      { op: "securityProfile", workload: web, profile: "baseline" },
      { op: "podSecurity", level: "baseline" },
    ])
    expect(profileChanges(web, "restricted", "baseline")).toStrictEqual([
      { op: "securityProfile", workload: web, profile: "restricted" },
    ])
  })
})
