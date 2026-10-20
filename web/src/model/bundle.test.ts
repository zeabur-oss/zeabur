import { describe, expect, test } from "bun:test"

import { writeBundle } from "@/model/bundle"

const write = ({ key, params }: { key: string; params?: Record<string, string> }) =>
  key === "diagnosis.crashLoop.cause.exitCode"
    ? `${params?.container} exits with ${params?.exitCode}.`
    : undefined

describe("evidence bundle", () => {
  test("a known sentence is written with its values", () => {
    expect(
      writeBundle("- `diagnosis.crashLoop.cause.exitCode` (container=api, exitCode=1)\n", write),
    ).toBe("- api exits with 1.\n")
  })

  test("an unknown sentence and plain code stay as they are", () => {
    const text = "`diagnosis.fromTheFuture` (a=1) and `kubectl get pods`"
    expect(writeBundle(text, write)).toBe(text)
  })
})
