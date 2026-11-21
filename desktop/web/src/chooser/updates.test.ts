import { describe, expect, it } from "bun:test"

import { answer, type Available } from "./updates"

const update: Available = {
  version: "0.3.0",
  current: "0.3.0-beta.1",

  install: "inPlace",
}

describe("update check answers", () => {
  it("offers a newer version", () => {
    expect(answer({ found: update })).toEqual({ kind: "available", update })
  })

  it("says the build is up to date when no newer version is found", () => {
    expect(answer({ found: null })).toEqual({ kind: "latest" })
  })

  it("tells a build without update checks apart", () => {
    expect(answer({ error: "notConfigured" })).toEqual({ kind: "failed", reason: "notConfigured" })
  })

  it("turns any other error into the short failure", () => {
    for (const error of [
      "failed",
      "Could not fetch a valid release JSON from the remote",
      new Error("x"),
    ])
      expect(answer({ error })).toEqual({ kind: "failed", reason: "failed" })
  })
})
