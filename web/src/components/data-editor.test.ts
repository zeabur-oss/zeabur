import { describe, expect, test } from "bun:test"

import { dataChangesOf, editAfterTyping, secretKeys } from "@/components/data-editor"

const secretYaml = `apiVersion: v1
kind: Secret
metadata:
  name: db
data:
  cert: /wCA
  password: aHVudGVyMg==
type: Opaque
`

describe("the key editor", () => {
  test("text values are shown decoded and binary ones kept as bytes", () => {
    const { text, binary } = secretKeys(secretYaml)
    expect(text).toEqual({ password: "hunter2" })
    expect([...(binary.cert ?? [])]).toEqual([0xff, 0x00, 0x80])
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
    const { binary } = secretKeys(secretYaml)
    expect(Object.keys(binary)).toEqual(["cert"])
    expect(dataChangesOf({ edits: { password: { op: "set", value: "new" } }, added: [] })).toEqual([
      { key: "password", op: "set", value: "new" },
    ])
  })

  test.each(["(hidden)", "••••••••"])("%s can be set on purpose", (value) => {
    const edit = editAfterTyping("old", value, false)
    expect(edit).toEqual({ op: "set", value })
    expect(dataChangesOf({ edits: { k: edit! }, added: [] })).toEqual([
      { key: "k", op: "set", value },
    ])
  })
})
