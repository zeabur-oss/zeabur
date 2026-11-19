import { describe, expect, test } from "bun:test"

import { imageProfile } from "@/model/pod-security"

describe("security profiles", () => {
  test("an image with a numeric user other than root is Strict, any other is Compatible", () => {
    for (const user of ["101", "1000:1000", "65532:0"])
      expect(imageProfile(user)).toBe("restricted")
    for (const user of ["", "0", "0:0", "root", "nginx", "nginx:nginx", "1000x"])
      expect(imageProfile(user)).toBe("baseline")
    expect(imageProfile(undefined)).toBe("baseline")
  })
})
