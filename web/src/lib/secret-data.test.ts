import { describe, expect, test } from "bun:test"

import { decodeSecretValue } from "@/lib/secret-data"

describe("a secret's value", () => {
  test("text decodes as UTF-8 and round trips", () => {
    const value = decodeSecretValue(
      btoa(String.fromCharCode(...new TextEncoder().encode("пароль ✓"))),
    )
    expect(value?.text).toBe("пароль ✓")
  })

  test("bytes that are not UTF-8 are binary, kept byte for byte", () => {
    const value = decodeSecretValue("/wCA")
    expect(value?.text).toBeUndefined()
    expect([...(value?.bytes ?? [])]).toEqual([0xff, 0x00, 0x80])
  })

  test("a leading byte order mark stays in the text", () => {
    expect(decodeSecretValue("77u/cGFzc3dvcmQ=")?.text).toBe("\uFEFFpassword")
  })

  test("a value that is not base64 is not decoded", () => {
    expect(decodeSecretValue("not base64!")).toBeUndefined()
  })
})
