import { describe, expect, test } from "bun:test"

import { decodeSecretValue, secretData } from "@/lib/secret-data"

const yaml = `apiVersion: v1
kind: Secret
metadata:
  name: db
  namespace: shop
  annotations:
    note: "data: no"
data:
  "1": MQ==
  empty: ""
  number: "1234"
  password: aHVudGVyMg==
  tls.key: /wCA
type: Opaque
`

describe("a secret's data from its YAML", () => {
  test("each key keeps its base64 value, quoted or not", () => {
    expect(secretData(yaml)).toEqual({
      "1": "MQ==",
      empty: "",
      number: "1234",
      password: "aHVudGVyMg==",
      "tls.key": "/wCA",
    })
  })

  test("a secret without data has no keys", () => {
    expect(
      secretData("apiVersion: v1\nkind: Secret\nmetadata:\n  name: db\ntype: Opaque\n"),
    ).toEqual({})
  })

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

  test("a value that is not base64 is not decoded", () => {
    expect(decodeSecretValue("not base64!")).toBeUndefined()
  })
})
