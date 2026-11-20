import { describe, expect, test } from "bun:test"

import { groups, lead, maximum, type NamespaceUsage } from "@/model/cluster-usage"

const MiB = 1024 * 1024

function namespace(name: string, over: Partial<NamespaceUsage> = {}): NamespaceUsage {
  return {
    origin: { apiVersion: "v1", kind: "Namespace", name: name },
    namespace: name,
    adopted: false,
    system: false,
    pods: 1,
    requests: { cpuMillicores: 100, memoryBytes: 128 * MiB },
    limits: { cpuMillicores: 0, memoryBytes: 0 },
    unlimited: { cpu: 1, memory: 1 },
    ...over,
  } as NamespaceUsage
}

describe("cluster usage", () => {
  test("a row leads with measured use, and with what it reserves before metrics answer", () => {
    const reservedOnly = namespace("shop")
    const measured = namespace("shop", {
      usage: { cpuMillicores: 40, memoryBytes: 300 * MiB, pods: 1 },
    })
    expect(lead(reservedOnly, "memory")).toBe(128 * MiB)
    expect(lead(measured, "memory")).toBe(300 * MiB)
  })

  test("projects come first and Kubernetes' own namespaces apart, each largest first", () => {
    const { projects, others, system } = groups(
      [
        namespace("kube-system", { system: true }),
        namespace("small", { adopted: true }),
        namespace("lab"),
        namespace("big", {
          adopted: true,
          requests: { cpuMillicores: 100, memoryBytes: 900 * MiB },
        }),
      ],
      "memory",
    )
    expect(projects.map((p) => p.namespace)).toEqual(["big", "small"])
    expect(others.map((p) => p.namespace)).toEqual(["lab"])
    expect(system.map((p) => p.namespace)).toEqual(["kube-system"])
  })

  test("without the namespaces' labels nothing counts as a project", () => {
    const { projects, others } = groups([namespace("shop", { adopted: undefined })], "cpu")
    expect(projects).toEqual([])
    expect(others.map((p) => p.namespace)).toEqual(["shop"])
  })

  test("the maximum is known only when every pod sets a limit", () => {
    const limits = { cpuMillicores: 1000, memoryBytes: 512 * MiB }
    const someUnlimited = namespace("shop", { limits, unlimited: { cpu: 0, memory: 1 } })
    const allLimited = namespace("shop", { limits, unlimited: { cpu: 0, memory: 0 } })
    expect(maximum(someUnlimited, "memory")).toBeUndefined()
    expect(maximum(allLimited, "memory")).toBe(512 * MiB)
  })
})
