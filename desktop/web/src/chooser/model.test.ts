import { describe, expect, it } from "bun:test"

import type { Auth, Context, ShellRead, Snapshot } from "./api"
import { authText, baseName, fileLabel, sourceFilter, visibleContexts } from "./model"

const t = ((key: string, ...args: unknown[]) => [key, ...args].join(" ")) as never

const done: ShellRead = { state: "done" }

const context = (name: string, extra: Partial<Context> = {}): Context => ({
  key: name,
  sourceId: "a",
  file: "/home/me/.kube/config",
  name,
  cluster: name,
  server: `https://${name}.example:6443`,
  user: "admin",
  namespace: null,
  current: false,
  auth: { state: "ok", method: "token" },
  ...extra,
})

const snapshot = (contexts: Context[]): Snapshot => ({
  sources: [],
  contexts,
  shell: { state: "done" },
})

describe("desktop model", () => {
  it("names a file by its last one or two path parts", () => {
    expect(baseName("C:\\Users\\me\\config")).toBe("config")
    expect(fileLabel("/home/me/team/aks.yaml")).toBe("team/aks.yaml")
  })

  it("explains each sign-in problem and stays silent when sign-in works", () => {
    const missing: Auth = { state: "missingEntry", what: "user", name: "bob" }
    expect(authText({ state: "ok", method: "token" }, done, t)).toBeNull()
    expect(authText(missing, done, t)).toBe("desktop.authMissingUser bob")
    expect(authText({ ...missing, what: "cluster" }, done, t)).toBe(
      "desktop.authMissingCluster bob",
    )
    expect(authText({ state: "fileMissing", path: "/a/b/c.pem" }, done, t)).toBe(
      "desktop.authFileMissing b/c.pem",
    )
    const plugin: Auth = { state: "pluginMissing", command: "aws", hint: null }
    expect(authText(plugin, done, t)).toBe("desktop.authPluginMissing aws")
    expect(authText(plugin, { state: "pending" }, t)).toBe("desktop.authLooking aws")
  })

  it("filters by source and search", () => {
    const snap = snapshot([
      context("dev"),
      context("prod"),
      context("lab", { sourceId: "b", user: null }),
    ])
    expect(visibleContexts(snap, "", null).map((c) => c.name)).toEqual(["dev", "prod", "lab"])
    expect(visibleContexts(snap, " LAB ", null).map((c) => c.name)).toEqual(["lab"])
    expect(visibleContexts(snap, "", "b").map((c) => c.name)).toEqual(["lab"])
  })

  it("drops the source filter once its source is removed", () => {
    const source = { id: "b" } as Snapshot["sources"][number]
    const snap = snapshot([context("dev")])
    expect(sourceFilter({ ...snap, sources: [source] }, "b")).toBe("b")
    expect(sourceFilter(snap, "b")).toBeNull()
  })
})
