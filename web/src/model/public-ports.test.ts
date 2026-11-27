import { describe, expect, test } from "bun:test"

import { externalOf, openPortChange, outside, portChoices, typeLabel } from "@/model/public-ports"
import type { Endpoint } from "@/model/service"

const workload = { kind: "Deployment", name: "web" } as const
const choice = { value: "web/8080", label: "web:8080 TCP", from: "web", port: 8080 }

describe("the external port of a public port", () => {
  test.each([
    ["", undefined],
    [" 12345 ", 12345],
    ["0", Number.NaN],
    ["65536", Number.NaN],
    ["80a", Number.NaN],
    ["1.5", Number.NaN],
  ])("%p reads as %p", (text, port) => {
    expect(externalOf(text)).toBe(port)
  })

  test("another port goes in the change, the same port or none leaves it out", () => {
    expect(openPortChange(workload, choice, 12345)).toEqual({
      op: "publicPort",
      workload,
      service: "web",
      port: 8080,
      external: 12345,
    })
    for (const external of [undefined, 8080]) {
      expect(openPortChange(workload, choice, external)).not.toHaveProperty("external")
    }
  })
})

test("a port that a public port opens is not offered again, whatever its external port", () => {
  const port = (n: number) => ({ port: n, protocol: "TCP" }) as Endpoint["ports"][number]
  const web = { name: "web", ports: [port(8080), port(9090)] } as Endpoint
  const opened = {
    name: "web-public-8080",
    type: "LoadBalancer",
    ports: [port(12345)],
    opens: { service: "web", port: 8080 },
  } as Endpoint
  expect(portChoices([web], [opened]).map((c) => c.port)).toEqual([9090])
})

describe("a Service by its Kubernetes type", () => {
  test.each([
    [{}, "ClusterIP", false],
    [{ type: "ClusterIP", clusterIP: "None" }, "Headless", false],
    [{ type: "NodePort" }, "NodePort", true],
    [{ type: "LoadBalancer" }, "LoadBalancer", true],
  ])("%p is %p, reachable from outside the cluster: %p", (fields, label, out) => {
    const e = { name: "web", ports: [], ...fields } as Endpoint
    expect(typeLabel(e)).toBe(label)
    expect(outside(e)).toBe(out)
  })
})
