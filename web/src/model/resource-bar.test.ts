import { describe, test, expect } from "bun:test"

import type { Schemas } from "@/api/client"
import { barShares, busiest, containerBar, largestNode } from "@/model/resource-bar"
import type { Service } from "@/model/service"

const MiB = 2 ** 20

describe("a resource bar", () => {
  test("spans from the request to the limit on a logarithmic scale up to the largest node", () => {
    // 10m to 10 cores is three decades: 100m is a third, 1 core two thirds.
    const shares = barShares("cpu", "100m", "1", 0.1, 10)
    expect(shares?.from).toBeCloseTo(1 / 3)
    expect(shares?.to).toBeCloseTo(2 / 3)
    expect(shares?.mark).toBeCloseTo(1 / 3)
  })

  test("runs from the left end without a request and to the right end without a limit", () => {
    expect(barShares("bytes", "", "", undefined, 16 * 1024 * MiB)).toStrictEqual({
      from: 0,
      to: 1,
      mark: undefined,
    })
  })

  test("reads what is typed, in any form the field takes", () => {
    const typed = barShares("bytes", "256 MiB", "1Gi", undefined, 16 * 1024 * MiB)
    const kube = barShares("bytes", "256Mi", "1024Mi", undefined, 16 * 1024 * MiB)
    expect(typed).toStrictEqual(kube)
    expect(barShares("cpu", "abc", "", undefined, 4)?.from).toBe(0)
  })

  test("keeps every mark on the bar, and the limit not left of the request", () => {
    const shares = barShares("cpu", "8", "2", 0.001, 4)
    expect(shares).toStrictEqual({ from: 1, to: 1, mark: 0 })
  })

  test("shows nothing without a node to scale to", () => {
    expect(barShares("cpu", "100m", "1", 0.1, undefined)).toBeUndefined()
  })
})

describe("a container's bar", () => {
  const largest = { cpuMillicores: 4000, memoryBytes: 8 * 1024 * MiB } as Schemas.Resources

  test("takes the dimension's sample and the largest node's amount", () => {
    const amounts = { cpuRequest: " 100m", cpuLimit: "1", memoryRequest: "", memoryLimit: "" }
    const bar = containerBar("cpu", amounts, { cpu: 0.2, memory: 300 * MiB }, largest)
    expect(bar).toMatchObject({ request: "100m", limit: "1", usage: 0.2, node: 4 })
    const memory = containerBar("bytes", amounts, undefined, largest)
    expect(memory).toMatchObject({ request: "", limit: "", node: 8 * 1024 * MiB })
    expect(memory?.usage).toBeUndefined()
  })

  test("is absent before the nodes are read", () => {
    expect(
      containerBar("cpu", undefined, undefined, { cpuMillicores: 4000 } as never),
    ).toBeUndefined()
    expect(
      containerBar(
        "cpu",
        { cpuRequest: "", cpuLimit: "", memoryRequest: "", memoryLimit: "" },
        undefined,
        undefined,
      ),
    ).toBeUndefined()
  })
})

describe("a container's usage", () => {
  test("comes from its busiest pod of this service", () => {
    const service = { pods: [{ name: "web-1" }, { name: "web-2" }] } as unknown as Service
    const pod = (name: string, cpu: number, memory: number) => ({
      pod: name,
      cpuMillicores: cpu,
      memoryBytes: memory,
      containers: [{ name: "web", cpuMillicores: cpu, memoryBytes: memory }],
      origin: { apiVersion: "metrics.k8s.io/v1beta1", kind: "PodMetrics", name },
    })
    const usage = {
      pods: [pod("web-1", 30, 200 * MiB), pod("web-2", 80, 100 * MiB), pod("api-1", 900, MiB)],
    } as unknown as Schemas.ProjectUsage
    expect(busiest(service, usage).get("web")).toStrictEqual({ cpu: 0.08, memory: 200 * MiB })
    expect(busiest(service, undefined).size).toBe(0)
  })
})

describe("the largest node", () => {
  test("takes CPU and memory each from the node with the most", () => {
    const small = { cpuMillicores: 2000, memoryBytes: 32 * 2 ** 30 }
    const wide = { cpuMillicores: 16000, memoryBytes: 8 * 2 ** 30 }
    expect(largestNode([small, wide])).toStrictEqual({
      cpuMillicores: 16000,
      memoryBytes: 32 * 2 ** 30,
    })
    expect(largestNode([])).toBeUndefined()
  })
})
