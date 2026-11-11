import { describe, expect, test } from "bun:test"

import { changesOf, editAfterTyping } from "@/components/data-editor"

describe("the key editor", () => {
  test("typing then clearing an unrevealed secret key sends no edit", () => {
    expect(editAfterTyping(undefined, "a")).toEqual({ op: "set", value: "a" })
    expect(editAfterTyping(undefined, "")).toBeUndefined()
  })

  test("a value equal to the loaded one is no edit, and an empty one is an edit of a revealed key", () => {
    expect(editAfterTyping("x", "x")).toBeUndefined()
    expect(editAfterTyping("x", "")).toEqual({ op: "set", value: "" })
  })

  test("changing one key sends only that key", () => {
    expect(changesOf({ edits: { b: { op: "set", value: "2" } }, added: [] })).toEqual([
      { key: "b", op: "set", value: "2" },
    ])
  })

  test.each(["(hidden)", "••••••••"])("%s can be set on purpose", (value) => {
    const edit = editAfterTyping("old", value)
    expect(edit).toEqual({ op: "set", value })
    expect(changesOf({ edits: { k: edit! }, added: [] })).toEqual([{ key: "k", op: "set", value }])
  })
})
