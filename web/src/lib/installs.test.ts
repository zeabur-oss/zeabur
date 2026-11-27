import { beforeEach, describe, expect, mock, test } from "bun:test"

import type { Schemas } from "@/api/client"

const applied: Schemas.ComponentRequest[] = []
let answer: () => Promise<{ plan: Schemas.ComponentPlan }>

void mock.module("@/api/zeabur", () => ({
  applyComponent: (_id: string, request: Schemas.ComponentRequest) => {
    applied.push(request)
    return answer()
  },
}))

const { dismissInstall, installOf, installs, startInstall } = await import("@/lib/installs")

const plan = (done: boolean, state: Schemas.ComponentStep["state"] = "ready") =>
  ({
    action: "install",
    catalogVersion: "v1",
    done,
    steps: [{ component: "velero", release: "velero", state, writes: [] }],
    checks: [],
    inputs: [],
    warnings: [],
    leftovers: [],
  }) as unknown as Schemas.ComponentPlan

const settled = () =>
  new Promise<void>((resolve) => {
    const stop = () => {
      if (!installs.get().some((i) => i.running)) resolve()
      else setTimeout(stop, 1)
    }
    stop()
  })

const job = {
  id: "velero",
  name: "Velero",
  action: "install" as const,
  values: { velero: { region: "x" } },
}

describe("installs", () => {
  beforeEach(() => {
    applied.length = 0
    installs.set([])
  })

  test("an install runs with no dialog and keeps its last plan when it is done", async () => {
    answer = () => Promise.resolve({ plan: plan(true) })
    startInstall(job, "v1")
    await settled()
    expect(applied).toEqual([
      { action: "install", catalogVersion: "v1", values: { velero: { region: "x" } } },
    ])
    expect(installOf(installs.get(), "velero")).toMatchObject({ running: false, plan: plan(true) })
  })

  test("a failed step stops the install, and the person can dismiss it", async () => {
    answer = () => Promise.resolve({ plan: plan(false, "failed") })
    startInstall(job, "v1")
    await settled()
    expect(applied).toHaveLength(1)
    dismissInstall("velero")
    expect(installs.get()).toEqual([])
  })

  test("a refused apply stops the install with the problem", async () => {
    answer = () => Promise.reject(new Error("forbidden"))
    startInstall(job, "v1")
    await settled()
    expect(installOf(installs.get(), "velero")?.error?.message).toBe("forbidden")
  })

  test("a running install is not dismissed or started twice", () => {
    answer = () => new Promise(() => undefined)
    startInstall(job, "v1")
    startInstall(job, "v1")
    dismissInstall("velero")
    expect(applied).toHaveLength(1)
    expect(installOf(installs.get(), "velero")?.running).toBe(true)
  })
})
