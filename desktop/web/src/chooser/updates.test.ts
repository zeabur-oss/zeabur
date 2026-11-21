import { describe, expect, it } from "bun:test"

import { settle, type Available } from "./updates"

const update: Available = {
  version: "0.3.0",
  current: "0.3.0-beta.1",
  install: "inPlace",
}

describe("an update check", () => {
  it("offers a newer version and opens its dialog", () => {
    expect(settle(null, { found: update })).toEqual({
      check: { kind: "found" },
      update,
      open: true,
    })
  })

  it("says the build is up to date when no newer version is found", () => {
    expect(settle(update, { found: null })).toEqual({
      check: { kind: "latest" },
      update: null,
      open: false,
    })
  })

  it("keeps the update an earlier check found when it fails", () => {
    expect(settle(update, { error: "failed" })).toMatchObject({
      check: { kind: "failed", reason: "failed" },
      update,
    })
  })

  it("tells a build without update checks apart", () => {
    expect(settle(null, { error: "notConfigured" }).check).toEqual({
      kind: "failed",
      reason: "notConfigured",
    })
  })

  it("turns any other error into the short failure", () => {
    for (const error of [
      "failed",
      "Could not fetch a valid release JSON from the remote",
      new Error("x"),
    ])
      expect(settle(null, { error }).check).toEqual({ kind: "failed", reason: "failed" })
  })
})
