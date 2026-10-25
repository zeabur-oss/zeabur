import { describe, expect, test } from "bun:test"

import type { Schemas } from "@/api/client"
import { templateRoom } from "@/model/template-room"

const GiB = 2 ** 30
const MiB = 2 ** 20

const idle = (memoryBytes: number, cpuMillicores = 2): Schemas["IdleUse"] => ({
  memoryBytes,
  cpuMillicores,
  environment: "single-node-amd64-v1.36.4",
  measuredAt: "2026-10-24",
})

type Resources = Schemas["Resources"]
const nodes = (totals: {
  allocatable: Resources
  usage?: Resources
  requests?: Resources
}): Schemas["NodeList"] => ({ totals }) as Schemas["NodeList"]

const allocatable = { memoryBytes: 8 * GiB, cpuMillicores: 4000 }

describe("templateRoom", () => {
  test("room when the idle use leaves a margin free", () => {
    const room = templateRoom(
      idle(300 * MiB),
      nodes({ allocatable, usage: { memoryBytes: 3 * GiB, cpuMillicores: 500 } }),
    )
    expect(room).toEqual({ verdict: "room", resource: "memory", use: 300 * MiB, free: 5 * GiB })
  })

  test("tight when the idle use leaves less than the margin of memory", () => {
    const room = templateRoom(
      idle(1 * GiB),
      nodes({ allocatable, usage: { memoryBytes: 6.5 * GiB, cpuMillicores: 500 } }),
    )
    expect(room?.verdict).toBe("tight")
    expect(room?.resource).toBe("memory")
    expect(room?.free).toBe(1.5 * GiB)
  })

  test("tight on CPU when memory is fine and CPU is not", () => {
    const room = templateRoom(
      idle(100 * MiB, 800),
      nodes({ allocatable, usage: { memoryBytes: 1 * GiB, cpuMillicores: 3000 } }),
    )
    expect(room).toMatchObject({ verdict: "tight", resource: "cpu", use: 800, free: 1000 })
  })

  test("requests above use count as taken, since the scheduler places pods by them", () => {
    const room = templateRoom(
      idle(1 * GiB),
      nodes({
        allocatable,
        usage: { memoryBytes: 2 * GiB, cpuMillicores: 500 },
        requests: { memoryBytes: 7 * GiB, cpuMillicores: 500 },
      }),
    )
    expect(room?.verdict).toBe("tight")
  })

  test("free never reads below zero when the nodes are over-committed", () => {
    const room = templateRoom(
      idle(10 * MiB),
      nodes({ allocatable, requests: { memoryBytes: 9 * GiB, cpuMillicores: 500 } }),
    )
    expect(room).toMatchObject({ verdict: "tight", free: 0 })
  })

  test.each([
    ["no measurement", undefined, nodes({ allocatable, usage: allocatable })],
    ["a null measurement", null, nodes({ allocatable, usage: allocatable })],
    ["unreadable nodes", idle(10 * MiB), undefined],
    ["neither use nor requests", idle(10 * MiB), nodes({ allocatable })],
  ])("no data with %s", (_name, measured, list) => {
    expect(templateRoom(measured, list)).toBeUndefined()
  })
})
