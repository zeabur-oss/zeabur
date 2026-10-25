import { describe, expect, test } from "bun:test"

import { imageProfile, templateSecurity } from "@/model/pod-security"

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

  test("an image with a numeric user other than root is Strict, any other is Compatible", () => {
    for (const user of ["101", "1000:1000", "65532:0"])
      expect(imageProfile(user)).toBe("restricted")
    for (const user of ["", "0", "0:0", "root", "nginx", "nginx:nginx", "1000x"])
      expect(imageProfile(user)).toBe("baseline")
    expect(imageProfile(undefined)).toBe("baseline")
  })
})
