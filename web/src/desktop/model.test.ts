import { describe, expect, it } from "bun:test"

import type { Auth, Context, Snapshot } from "./api"
import {
  authText,
  baseName,
  fileLabel,
  parseScenario,
  recentContexts,
  reachView,
  sourceFilter,
  visibleContexts,
} from "./model"

const t = ((key: string, ...args: unknown[]) => [key, ...args].join(" ")) as never

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
  sameNameElsewhere: 0,
  auth: { state: "ok", method: "token" },
  ...extra,
})

const snapshot = (contexts: Context[], extra: Partial<Snapshot> = {}): Snapshot => ({
  sources: [],
  contexts,
  favourites: [],
  recents: [],
  shellPathUsed: false,
  ...extra,
})

describe("desktop model", () => {
  it("names a file by its last one or two path parts", () => {
    expect(baseName("C:\\Users\\me\\config")).toBe("config")
    expect(fileLabel("/home/me/team/aks.yaml")).toBe("team/aks.yaml")
  })

  it("shows a pending check until the reach is known", () => {
    expect(reachView(undefined, t)).toEqual({ variant: "idle", text: "desktop.reachChecking" })
    expect(reachView({ state: "reachable", millis: 12 }, t)).toEqual({
      variant: "ok",
      text: "desktop.reachOk 12 ms",
    })
    expect(reachView({ state: "timedOut" }, t).variant).toBe("warn")
    expect(reachView({ state: "refused" }, t).variant).toBe("fail")
  })

  it("explains each sign-in problem and stays silent when sign-in works", () => {
    const missing: Auth = { state: "missingEntry", what: "user", name: "bob" }
    expect(authText({ state: "ok", method: "token" }, t)).toBeNull()
    expect(authText(missing, t)).toBe("desktop.authMissingUser bob")
    expect(authText({ ...missing, what: "cluster" }, t)).toBe("desktop.authMissingCluster bob")
    expect(authText({ state: "fileMissing", path: "/a/b/c.pem" }, t)).toBe(
      "desktop.authFileMissing b/c.pem",
    )
    expect(authText({ state: "pluginMissing", command: "aws", hint: null }, t)).toBe(
      "desktop.authPluginMissing aws",
    )
    expect(authText({ state: "shellPathOnly", command: "aws" }, t)).toBe(
      "desktop.authShellPath aws",
    )
  })

  it("reads a scenario and skips empty keys", () => {
    expect(parseScenario("lang=en;q=prod;drop")).toEqual({ lang: "en", q: "prod", drop: "" })
    expect(parseScenario(null)).toEqual({})
  })

  it("filters by source and search, and lists favourites first", () => {
    const snap = snapshot(
      [context("dev"), context("prod"), context("lab", { sourceId: "b", user: null })],
      { favourites: ["prod"] },
    )
    expect(visibleContexts(snap, "", null).map((c) => c.name)).toEqual(["prod", "dev", "lab"])
    expect(visibleContexts(snap, " LAB ", null).map((c) => c.name)).toEqual(["lab"])
    expect(visibleContexts(snap, "", "b").map((c) => c.name)).toEqual(["lab"])
  })

  it("drops the source filter once its source is removed", () => {
    const source = { id: "b" } as Snapshot["sources"][number]
    const snap = snapshot([context("dev")])
    expect(sourceFilter({ ...snap, sources: [source] }, "b")).toBe("b")
    expect(sourceFilter(snap, "b")).toBeNull()
  })

  it("lists recents only without a search or source filter", () => {
    const snap = snapshot([context("dev"), context("prod")], { recents: ["prod", "gone"] })
    expect(recentContexts(snap, "", null).map((c) => c.name)).toEqual(["prod"])
    expect(recentContexts(snap, "d", null)).toEqual([])
    expect(recentContexts(snap, "", "a")).toEqual([])
  })
})
