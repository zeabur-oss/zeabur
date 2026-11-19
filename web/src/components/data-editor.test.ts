import { describe, expect, test } from "bun:test"

import { dataChangesOf, editAfterTyping, secretKeys } from "@/components/data-editor"
import { nextRowId } from "@/lib/row-id"

const secretData = { cert: "/wCA", password: "aHVudGVyMg==" }

describe("the key editor", () => {
  test("text values are shown decoded and binary ones kept as bytes", () => {
    const { text, binary } = secretKeys(secretData)
    expect(text).toEqual({ password: "hunter2" })
    expect([...(binary.cert ?? [])]).toEqual([0xff, 0x00, 0x80])
  })

  test("a text key named constructor is text, not binary", () => {
    const { text, binary } = secretKeys(
      JSON.parse('{"constructor": "YQ=="}') as Record<string, string>,
    )
    expect(text["constructor"]).toBe("a")
    expect(Object.hasOwn(binary, "constructor")).toBe(false)
    expect(binary["constructor"]).toBeUndefined()
  })

  test("typing then clearing a hidden value sends no edit", () => {
    expect(editAfterTyping("hunter2", "a", true)).toEqual({ op: "set", value: "a" })
    expect(editAfterTyping("hunter2", "", true)).toBeUndefined()
  })

  test("a value equal to the stored one is no edit; clearing a shown value is an edit", () => {
    expect(editAfterTyping("hunter2", "hunter2", true)).toBeUndefined()
    expect(editAfterTyping("hunter2", "hunter2", false)).toBeUndefined()
    expect(editAfterTyping("hunter2", "", false)).toEqual({ op: "set", value: "" })
  })

  test("changing one key sends only that key, and an untouched binary key is not sent", () => {
    const { binary } = secretKeys(secretData)
    expect(Object.keys(binary)).toEqual(["cert"])
    expect(dataChangesOf({ edits: { password: { op: "set", value: "new" } }, added: [] })).toEqual([
      { key: "password", op: "set", value: "new" },
    ])
  })
})

describe("keys added in the dialog", () => {
  test("each added row has its own id, so removing one keeps the others' input", () => {
    expect(nextRowId()).not.toBe(nextRowId())
  })
})
