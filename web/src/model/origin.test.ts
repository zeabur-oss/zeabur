import { describe, expect, test } from "bun:test"

import type { Schemas } from "@/api/client"
import { originOf } from "@/model/origin"

const diagnosis = (fields: Partial<Schemas.Diagnosis>) =>
  ({ kind: "crashLoop", cause: { key: "diagnosis.crashLoop.cause.exitCode" }, ...fields }) as never

describe("originOf", () => {
  test("takes the server's fault when there is one", () => {
    expect(
      originOf(
        diagnosis({ kind: "unreachable", fault: { origin: "component", fixer: "clusterAdmin" } }),
      ),
    ).toStrictEqual({ origin: "component", fixer: "clusterAdmin" })
  })

  test("falls back to the kind and cause", () => {
    expect(originOf(diagnosis({}))).toStrictEqual({ origin: "code", fixer: "you" })
    expect(
      originOf(
        diagnosis({ kind: "imagePull", cause: { key: "diagnosis.imagePull.cause.rateLimited" } }),
      ),
    ).toStrictEqual({ origin: "upstream", fixer: "you" })
  })
})
