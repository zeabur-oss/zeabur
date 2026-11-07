import { describe, expect, test } from "bun:test"

import { readBuildLog, stepSeconds } from "@/model/build-steps"

const at = (s: number) => new Date(Date.UTC(2026, 9, 22, 0, 0, s)).toISOString()

const vertex = (vertex: number, name: string) => ({
  started: (s: number) => JSON.stringify({ vertex, name, event: "started", time: at(s) }),
  log: (text: string, stream = "stdout") =>
    JSON.stringify({ vertex, name, event: "log", stream, text }),
  done: (seconds: number) => JSON.stringify({ vertex, name, event: "done", seconds }),
  cached: () => JSON.stringify({ vertex, name, event: "cached" }),
  error: (seconds: number, message: string) =>
    JSON.stringify({ vertex, name, event: "error", seconds, message }),
})

const definition = vertex(1, "[internal] load build definition from Dockerfile")
const from = vertex(2, "[1/3] FROM docker.io/library/node:22-alpine")
const npm = vertex(3, "[2/3] RUN npm ci")
const copy = vertex(4, "[3/3] COPY . .")
const exporting = vertex(5, "exporting to image")

const rows = (lines: string[]) =>
  readBuildLog(lines).steps.map((s) => ({ kind: s.kind, name: s.name, state: s.state }))

describe("readBuildLog", () => {
  test("a cached build shows each step cached, BuildKit's own work folded around them", () => {
    const log = readBuildLog([
      definition.started(0),
      definition.done(0.1),
      from.started(1),
      from.cached(),
      npm.started(1),
      npm.cached(),
      exporting.started(2),
      exporting.done(1.5),
    ])
    expect(log.steps.map((s) => [s.kind, s.state])).toEqual([
      ["prepare", "done"],
      ["own", "cached"],
      ["own", "cached"],
      ["export", "done"],
    ])
    expect(log.failed).toBeUndefined()
  })

  test("a successful build keeps each step's lines and time", () => {
    const log = readBuildLog([
      npm.started(1),
      npm.log("added 312 packages"),
      npm.log("npm warn deprecated glob@7", "stderr"),
      npm.done(18.4),
      copy.started(20),
      copy.done(0.5),
    ])
    const [ci, last] = log.steps
    expect(ci?.name).toBe("RUN npm ci")
    expect(ci?.lines).toEqual([
      { text: "added 312 packages", stderr: false },
      { text: "npm warn deprecated glob@7", stderr: true },
    ])
    expect(ci && stepSeconds(ci, 0)).toBe(18.4)
    expect(last?.state).toBe("done")
  })

  test("a failed build names the step that failed, with its time and error", () => {
    const log = readBuildLog([
      from.started(0),
      from.cached(),
      npm.started(1),
      npm.log("npm error code ERESOLVE"),
      npm.error(12.4, "exit code: 1"),
      JSON.stringify({ event: "message", text: "error: failed to solve: exit code: 1" }),
    ])
    expect(log.failed).toEqual({ name: "RUN npm ci", seconds: 12.4, message: "exit code: 1" })
    expect(rows([npm.started(1), npm.error(1, "x")])).toEqual([
      { kind: "own", name: "RUN npm ci", state: "failed" },
    ])
    expect(log.other).toEqual(["error: failed to solve: exit code: 1"])
  })

  test("a failure in BuildKit's own work fails its folded row under the step's name", () => {
    const metadata = vertex(1, "[internal] load metadata for docker.io/library/nod:22")
    const log = readBuildLog([metadata.started(0), metadata.error(0.8, "not found")])
    expect(log.steps[0]).toMatchObject({ kind: "prepare", state: "failed", error: "not found" })
    expect(log.failed?.name).toBe("[internal] load metadata for docker.io/library/nod:22")
  })

  test("a step's name drops BuildKit's [n/m] counter and keeps the rest", () => {
    expect(rows([vertex(9, "[12/14] RUN echo [1/2]").started(0)])[0]?.name).toBe("RUN echo [1/2]")
    expect(rows([definition.started(0)])[0]?.kind).toBe("prepare")
  })

  test("a running step counts its time from its start", () => {
    const log = readBuildLog([npm.started(1), npm.log("fetching")])
    const step = log.steps[0]
    expect(step?.state).toBe("running")
    expect(step && stepSeconds(step, Date.parse(at(11)))).toBe(10)
  })

  test("Railpack's second run continues the same list", () => {
    const install = vertex(3, "npm ci")
    const second = vertex(7, "[2/2] COPY --chown=1000 /app /app")
    expect(rows([install.started(0), install.done(3), second.started(4), second.done(1)])).toEqual([
      { kind: "own", name: "npm ci", state: "done" },
      { kind: "own", name: "COPY --chown=1000 /app /app", state: "done" },
    ])
  })

  test("lines that are not events, from an older builder, stay as they are", () => {
    const log = readBuildLog(["#1 [internal] load build definition", "#1 DONE 0.1s", "{not json"])
    expect(log.steps).toEqual([])
    expect(log.other).toEqual(["#1 [internal] load build definition", "#1 DONE 0.1s", "{not json"])
  })
})
