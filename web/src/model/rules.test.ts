import { describe, expect, it } from "vitest"
import { rules } from "@/api/rules"
import { isWholeNumber } from "@/model/rules"

describe("isWholeNumber", () => {
  const scale = rules["Change.scale.replicas"]

  it("takes the limits of the generated rules", () => {
    expect(isWholeNumber("0", scale)).toBe(true)
    expect(isWholeNumber(String(scale.maximum), scale)).toBe(true)
    expect(isWholeNumber(String(scale.maximum + 1), scale)).toBe(false)
  })

  it("refuses what is not a whole number", () => {
    for (const value of ["", "-1", "1.5", "two"]) expect(isWholeNumber(value, scale)).toBe(false)
  })

  it("is the same limit as adding a service", () => {
    expect(scale.maximum).toBe(rules["ImageServiceChange.replicas"].maximum)
  })
})
