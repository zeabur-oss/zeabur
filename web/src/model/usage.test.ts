import { describe, test, expect } from "bun:test"

import type { Pod, Service } from "@/model/service"
import {
  colours,
  measured,
  notesFor,
  podsAtEnd,
  serviceUsage,
  sumUsage,
  templateAmounts,
  total,
  type ProjectUsage,
} from "@/model/usage"

const pod = (name: string, extra: Partial<Pod> = {}) =>
  ({ name, phase: "Running", ready: true, restarts: 0, ...extra }) as Pod

const service = (pods: Pod[], resources: object, kind = "Deployment") =>
  ({
    kind,
    name: "web",
    pods,
    containers: [{ name: "web", resources }],
  }) as unknown as Service

const usage = (pods: [string, number, number][]): ProjectUsage =>
  ({
    id: "projects/shop/usage",
    state: "available",
    unreadable: [],
    pods: pods.map(([name, cpu, memory]) => ({
      pod: name,
      cpuMillicores: cpu,
      memoryBytes: memory,
      containers: [],
      origin: { kind: "PodMetrics", name },
    })),
  }) as unknown as ProjectUsage

const MiB = 2 ** 20

describe("requests and limits", () => {
  test("a limit is known only when every container sets one", () => {
    const s = {
      ...service([], {}),
      containers: [
        {
          name: "a",
          resources: { requests: { cpuMillicores: 100 }, limits: { memoryBytes: 256 * MiB } },
        },
        { name: "b", resources: { requests: { cpuMillicores: 50 } } },
        { name: "init", init: true, resources: { limits: { memoryBytes: 1024 * MiB } } },
      ],
    } as unknown as Service
    const { requests, limits } = templateAmounts(s)
    expect(requests.cpu).toBe(150)
    expect(limits.memory).toBe(undefined)
  })
})

describe("live usage", () => {
  const web = service([pod("web-1"), pod("web-2", { ready: false })], {
    requests: { cpuMillicores: 100, memoryBytes: 128 * MiB },
    limits: { memoryBytes: 256 * MiB },
  })

  test("a replica metrics-server has no sample of counts in no total", () => {
    const rows = measured(web, usage([["web-1", 40, 100 * MiB]]))
    expect(sumUsage(rows)).toStrictEqual({ cpu: 40, memory: 100 * MiB, pods: 1 })
    expect(
      notesFor(rows[1]!, { memory: 128 * MiB }, { memory: 256 * MiB }).map((n) => n.kind),
    ).toStrictEqual(["no-sample"])
  })

  test("notes come worst first, from the limits, the requests and the last stop", () => {
    const limits = { cpu: 100, memory: 256 * MiB }
    const near = notesFor(
      { pod: pod("w", { reason: "OOMKilled" }), cpu: 95, memory: 240 * MiB },
      { memory: 128 * MiB },
      limits,
    )
    expect(near.map((n) => n.kind)).toStrictEqual(["oom", "memory-near-limit", "cpu-at-limit"])
    // Above the request matters only without a limit: with one, the limit is the wall.
    const over = notesFor({ pod: pod("w"), cpu: 10, memory: 200 * MiB }, { memory: 128 * MiB }, {})
    expect(over.map((n) => n.kind)).toStrictEqual(["memory-over-request"])
    expect(notesFor({ pod: pod("w"), cpu: 10, memory: 200 * MiB }, {}, limits)).toStrictEqual([])
  })

  test("the services table shows a service only with a measured replica", () => {
    expect(serviceUsage(web, usage([["web-1", 40, 240 * MiB]]))).toStrictEqual({
      cpu: 40,
      memory: 240 * MiB,
      pods: 1,
      nearLimit: true,
    })
    expect(serviceUsage(web, usage([]))).toBe(undefined)
    expect(serviceUsage(service([pod("job-1")], {}, "CronJob"), usage([]))).toBe(undefined)
  })
})

describe("history", () => {
  const series = [
    { pod: "web-a", values: [1, null, 3] },
    { pod: "web-b", values: [2, null, null] },
  ]

  test("the total sums what was measured and keeps gaps", () => {
    expect(total(series)).toStrictEqual({ pod: "total", values: [3, null, 3] })
  })

  test("the total's reserved and maximum lines count the replicas at the end", () => {
    expect(podsAtEnd(series)).toBe(1)
    expect(podsAtEnd([{ pod: "a", values: [1, null] }])).toBe(1)
    expect(podsAtEnd([])).toBe(0)
  })

  test("current replicas take the palette in order, replaced ones the grey", () => {
    const all = [...series, { pod: "web-old", values: [] }, { pod: "web-c", values: [] }]
    expect(colours(all, new Set(["web-old"]))).toStrictEqual([
      "var(--chart-1)",
      "var(--chart-2)",
      "var(--chart-5)",
      "var(--chart-3)",
    ])
  })
})
