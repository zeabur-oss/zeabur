import { describe, test, expect } from "bun:test"

import type { Schemas } from "@/api/client"
import { busiest, usageWarnings } from "@/model/usage-warning"
import type { Service } from "@/model/service"

const MiB = 2 ** 20

describe("warnings for a container's usage", () => {
  test("memory near its limit says the container is close to a restart", () => {
    expect(usageWarnings("bytes", "", "512Mi", 470 * MiB).nearLimit).toStrictEqual({
      limit: 512 * MiB,
    })
    expect(usageWarnings("bytes", "", "512Mi", 300 * MiB).nearLimit).toBeUndefined()
  })

  test("CPU near its limit is not a warning, as CPU is slowed down, not restarted", () => {
    expect(usageWarnings("cpu", "", "1", 0.95).nearLimit).toBeUndefined()
  })

  test("usage above the request says the container uses more than it reserves", () => {
    expect(usageWarnings("cpu", "100m", "", 0.3).aboveRequest).toStrictEqual({ request: 0.1 })
    expect(usageWarnings("cpu", "500m", "", 0.3).aboveRequest).toBeUndefined()
    expect(usageWarnings("bytes", "256Mi", "", 300 * MiB).aboveRequest).toStrictEqual({
      request: 256 * MiB,
    })
  })

  test("follows what is typed, in any form the field takes", () => {
    expect(usageWarnings("bytes", "256 MiB", "", 300 * MiB)).toStrictEqual(
      usageWarnings("bytes", "256Mi", "", 300 * MiB),
    )
  })

  test("says nothing without usage, without an amount or with text that is no amount", () => {
    expect(usageWarnings("bytes", "1Mi", "1Mi", undefined)).toStrictEqual({})
    expect(usageWarnings("bytes", "", "", 300 * MiB)).toStrictEqual({})
    expect(usageWarnings("cpu", "abc", "xyz", 5)).toStrictEqual({})
  })
})

describe("the busiest pod of a service", () => {
  const service = { pods: [{ name: "a" }, { name: "b" }] } as unknown as Service
  const pod = (name: string, cpuMillicores: number, memoryBytes: number) => ({
    pod: name,
    containers: [{ name: "app", cpuMillicores, memoryBytes }],
  })

  test("takes each container's most from the service's own pods", () => {
    const usage = {
      pods: [pod("a", 100, 5 * MiB), pod("b", 300, 2 * MiB), pod("other", 900, 900 * MiB)],
    } as unknown as Schemas.ProjectUsage
    expect(busiest(service, usage).get("app")).toStrictEqual({ cpu: 0.3, memory: 5 * MiB })
  })

  test("is empty without usage", () => {
    expect(busiest(service, undefined).get("app")).toBeUndefined()
  })
})
