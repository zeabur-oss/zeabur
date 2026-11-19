import { describe, expect, test } from "bun:test"

import { numberOfText } from "@/components/form-field"

describe("a number field's text", () => {
  test("an empty field is not a number, so it is not written as 0", () => {
    expect(numberOfText("")).toBeNaN()
    expect(numberOfText("  ")).toBeNaN()
  })

  test("digits are the number, and anything else is refused by the owner's checks", () => {
    expect(numberOfText("7")).toBe(7)
    expect(numberOfText(" 14 ")).toBe(14)
    expect(numberOfText("1.5")).toBe(1.5)
    expect(numberOfText("7d")).toBeNaN()
  })
})
