import { describe, expect, test } from "bun:test"

import type { Schemas } from "@/api/client"
import {
  enters,
  nodeShellUrl,
  shellHealth,
  shellsOn,
  shellWord,
  subjectName,
} from "@/model/node-shell"

const shell = (patch: Partial<Schemas["NodeShellView"]>): Schemas["NodeShellView"] => ({
  name: "zeabur-node-shell-vps-1-a1b2c3",
  node: "vps-1",
  mine: true,
  phase: "running",
  ...patch,
})

describe("node shells", () => {
  test("the terminal is the node shell's own endpoint, over TLS when the page is", () => {
    expect(nodeShellUrl({ protocol: "https:", host: "z.example" }, "p/1")).toBe(
      "wss://z.example/api/v1/cluster/node-shells/p%2F1/terminal",
    )
    expect(nodeShellUrl({ protocol: "http:", host: "127.0.0.1:5173" }, "p")).toBe(
      "ws://127.0.0.1:5173/api/v1/cluster/node-shells/p/terminal",
    )
  })

  test("an ended shell is grey, a deadline included; a stuck start warns", () => {
    expect(shellHealth(shell({ phase: "ended", reason: "DeadlineExceeded" }))).toBe("idle")
    expect(shellWord(shell({ phase: "ended", reason: "DeadlineExceeded" }))).toBe("deadline")
    expect(shellWord(shell({ phase: "ended", reason: "Completed" }))).toBe("ended")
    expect(shellHealth(shell({ phase: "starting" }))).toBe("progress")
    expect(shellHealth(shell({ phase: "starting", reason: "ContainerCreating" }))).toBe("progress")
    expect(shellWord(shell({ phase: "starting", reason: "ErrImagePull" }))).toBe("stuck")
    expect(shellHealth(shell({}))).toBe("ok")
  })

  test("only the opener enters, and only a running shell", () => {
    expect(enters(shell({}))).toBe(true)
    expect(enters(shell({ mine: false }))).toBe(false)
    expect(enters(shell({ phase: "ended" }))).toBe(false)
  })

  test("a node's shells, and subjects by namespace", () => {
    const list = [shell({}), shell({ name: "other", node: "vps-2" })]
    expect(shellsOn(list, "vps-2").map((s) => s.name)).toStrictEqual(["other"])
    expect(subjectName({ kind: "ServiceAccount", name: "ci", namespace: "build" })).toBe("build/ci")
    expect(subjectName({ kind: "User", name: "alice" })).toBe("alice")
  })
})
