import { describe, test, expect } from "bun:test"

import type { Problem } from "@/api/client"
import {
  execRole,
  nextState,
  parseFrame,
  reconnectsWithRights,
  terminalUrl,
  type TerminalEvent,
  type TerminalState,
} from "@/model/terminal"

const run = (...events: TerminalEvent[]): TerminalState =>
  events.reduce<TerminalState>(nextState, { phase: "connecting" })

const frame = (text: string): TerminalEvent => {
  const parsed = parseFrame(text)
  if (!parsed) throw new Error(text)
  return { type: "frame", frame: parsed }
}

const refused: Problem = {
  type: "/problems/forbidden",
  title: "You do not have permission",
  status: 403,
  detail: "The cluster does not let you read or change a terminal in pod web-1.",
  kind: "forbidden",
  summary: { key: "problem.summary.forbidden" },
  fix: "Ask a cluster administrator for access.",
  denied: { verb: "get", group: "", resource: "pods/exec", name: "web-1", namespace: "shop" },
}

describe("terminal states", () => {
  test("ready opens the terminal; exit ends it and the close after it changes nothing", () => {
    const ready = frame(`{"type":"ready","container":"web"}`)
    expect(run(ready)).toStrictEqual({ phase: "open", container: "web" })
    expect(
      run(ready, frame(`{"type":"exit","code":3}`), { type: "close", code: 1000 }),
    ).toStrictEqual({
      phase: "exited",
      code: 3,
      reason: undefined,
      message: undefined,
    })
  })

  test("an image without a shell ends without a code", () => {
    const state = run(
      frame(`{"type":"ready","container":"web"}`),
      frame(`{"type":"exit","reason":"command-not-found"}`),
    )
    expect(state.phase === "exited" && state.reason).toBe("command-not-found")
  })

  test("a refused terminal reconnects once admin rights become active, whatever granted them", () => {
    const error = run(frame(JSON.stringify({ type: "error", problem: refused })))
    expect(reconnectsWithRights(error, "available", "active")).toBe(true)
    expect(reconnectsWithRights(error, undefined, "active")).toBe(true)
    // No change of rights, or rights that went away: the refusal stands.
    expect(reconnectsWithRights(error, "active", "active")).toBe(false)
    expect(reconnectsWithRights(error, "available", "available")).toBe(false)
    expect(reconnectsWithRights(error, "active", "available")).toBe(false)
    // Only a refusal is retried: not another error, nor a terminal that runs or ended.
    const failed = run(
      frame(JSON.stringify({ type: "error", problem: { ...refused, kind: "unavailable" } })),
    )
    expect(reconnectsWithRights(failed, "available", "active")).toBe(false)
    const open = run(frame(`{"type":"ready","container":"web"}`))
    expect(reconnectsWithRights(open, "available", "active")).toBe(false)
    expect(reconnectsWithRights(run({ type: "close", code: 4000 }), "available", "active")).toBe(
      false,
    )
  })

  test("an error keeps its problem through the close", () => {
    const error = frame(JSON.stringify({ type: "error", problem: refused }))
    expect(run(error, { type: "close", code: 1000 })).toStrictEqual({
      phase: "error",
      problem: refused,
    })
  })

  test("the server's own closes say why; any other close is a lost connection", () => {
    const ready = frame(`{"type":"ready","container":"web"}`)
    expect(run(ready, { type: "close", code: 4000 })).toStrictEqual({
      phase: "closed",
      why: "expired",
    })
    expect(run(ready, { type: "close", code: 4001 })).toStrictEqual({
      phase: "closed",
      why: "idle",
    })
    expect(run({ type: "close", code: 1006 })).toStrictEqual({ phase: "closed", why: "lost" })
  })

  test("unknown or malformed frames are ignored", () => {
    expect(parseFrame(`{"type":"later-addition"}`)).toBe(undefined)
    expect(parseFrame("not json")).toBe(undefined)
    expect(parseFrame("3")).toBe(undefined)
  })
})

describe("terminal address and access", () => {
  test("the address is the page's origin, ws or wss by its scheme", () => {
    expect(
      terminalUrl({ protocol: "https:", host: "engawa.example" }, "shop", "web-1", "web"),
    ).toBe("wss://engawa.example/api/v1/projects/shop/pods/web-1/exec?container=web")
    expect(
      terminalUrl({ protocol: "http:", host: "127.0.0.1:8080" }, "a b", "p/1", undefined),
    ).toBe("ws://127.0.0.1:8080/api/v1/projects/a%20b/pods/p%2F1/exec")
  })

  test("the Role to ask for grants get and create on pods/exec in the namespace only", () => {
    const yaml = execRole("shop", "alice@example.com")
    expect(yaml).toMatch(/kind: Role\n/)
    expect(yaml).toMatch(/resources: \["pods\/exec"\]\n {4}verbs: \["get", "create"\]/)
    expect(yaml).toMatch(/namespace: shop/)
    expect(yaml).toMatch(/name: "alice@example.com"/)
    expect(yaml).not.toMatch(/ClusterRole/)
  })
})
