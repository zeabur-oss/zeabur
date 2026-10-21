import { describe, expect, it } from "bun:test"
import { isLongList, LONG_LIST } from "@/model/lists"

describe("isLongList", () => {
  it("leaves empty, single and short lists alone", () => {
    expect(isLongList(0)).toBe(false)
    expect(isLongList(1)).toBe(false)
    expect(isLongList(LONG_LIST)).toBe(false)
  })
  it("bounds a list past the threshold", () => {
    expect(isLongList(LONG_LIST + 1)).toBe(true)
    expect(isLongList(80)).toBe(true)
    expect(isLongList(11, 10)).toBe(true)
  })
})
