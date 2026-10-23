import { beforeEach, describe, expect, test } from "bun:test"

import {
  closeTab,
  commandTyped,
  dock,
  isDockShortcut,
  openDatabaseShell,
  openKubectl,
  openShell,
} from "@/lib/dock"

describe("the dock", () => {
  beforeEach(() => dock.set({ open: false, tabs: [], focus: 0 }))

  test("Ctrl+` toggles; a backtick alone, or with another modifier, types", () => {
    const key = (over: Partial<KeyboardEvent>) => ({
      ctrlKey: false,
      metaKey: false,
      altKey: false,
      shiftKey: false,
      code: "Backquote",
      key: "`",
      ...over,
    })
    expect(isDockShortcut(key({ ctrlKey: true }))).toBe(true)
    // A layout that prints ^ on that key.
    expect(isDockShortcut(key({ ctrlKey: true, key: "^" }))).toBe(true)
    expect(isDockShortcut(key({}))).toBe(false)
    expect(isDockShortcut(key({ ctrlKey: true, shiftKey: true, key: "~" }))).toBe(false)
    expect(isDockShortcut(key({ metaKey: true }))).toBe(false)
    expect(isDockShortcut(key({ ctrlKey: true, altKey: true }))).toBe(false)
  })

  test("a service's terminal opens once; asking again shows it", () => {
    openShell("shop", { kind: "Deployment", name: "api" })
    openKubectl("shop")
    openShell("shop", { kind: "Deployment", name: "api" }, "api-1")
    const state = dock.get()
    expect(state.open).toBe(true)
    expect(state.tabs.map((tab) => tab.id)).toStrictEqual([
      "shell:shop/Deployment/api",
      "kubectl:shop",
    ])
    expect(state.active).toBe("shell:shop/Deployment/api")
    expect(state.tabs[0]).toMatchObject({ pod: "api-1" })
  })

  test("a database's terminal is a shell tab that names the database", () => {
    const orders = { engine: "postgresql", kind: "Cluster", name: "orders" }
    openDatabaseShell("shop", orders)
    openDatabaseShell("shop", orders, "orders-2")
    expect(dock.get().tabs).toStrictEqual([
      {
        id: "shell:shop/Cluster/orders",
        kind: "shell",
        project: "shop",
        service: { kind: "Cluster", name: "orders" },
        pod: "orders-2",
        database: { engine: "postgresql", name: "orders" },
      },
    ])
  })

  test("asking for kubectl again counts the ask; opening from an old link does not", () => {
    openKubectl("shop", { ask: false })
    expect(dock.get().tabs[0]).toMatchObject({ asked: 0 })
    openKubectl("shop")
    expect(dock.get().tabs[0]).toMatchObject({ asked: 1 })
  })

  test("a command waits on the kubectl tab until its shell has it; a plain ask keeps it", () => {
    openKubectl("shop", { command: "kubectl get pods" })
    expect(dock.get().tabs[0]).toMatchObject({ asked: 1, command: "kubectl get pods" })
    openKubectl("shop")
    expect(dock.get().tabs[0]).toMatchObject({ asked: 2, command: "kubectl get pods" })
    openKubectl("shop", { command: "helm list" })
    expect(dock.get().tabs[0]).toMatchObject({ asked: 3, command: "helm list" })
    commandTyped("kubectl:shop")
    expect(dock.get().tabs[0]).toMatchObject({ asked: 3, command: undefined })
    expect(dock.get()).toMatchObject({ open: true, active: "kubectl:shop" })
  })

  test("closing shows the tab beside it and leaves the dock open after the last", () => {
    openShell("shop", { kind: "Deployment", name: "api" })
    openShell("shop", { kind: "Deployment", name: "worker" })
    openKubectl("shop")
    closeTab("kubectl:shop")
    expect(dock.get().active).toBe("shell:shop/Deployment/worker")
    closeTab("shell:shop/Deployment/api")
    expect(dock.get().active).toBe("shell:shop/Deployment/worker")
    closeTab("shell:shop/Deployment/worker")
    expect(dock.get()).toMatchObject({ open: true, tabs: [], active: undefined })
  })
})
