import { describe, expect, test } from "bun:test"

import { addressRows, hasNodePort, outside, typeLabel } from "@/model/port-addresses"
import type { Endpoint } from "@/model/service"

type Port = Endpoint["ports"][number]
const port = (fields: Partial<Port> = {}) => ({ port: 80, protocol: "TCP", ...fields }) as Port
const service = (fields: Partial<Endpoint>, p: Port = port()) =>
  ({ name: "web", ports: [p], ...fields }) as Endpoint
const inCluster = [
  { kind: "project", address: "web:80" },
  { kind: "cluster", address: "web.shop.svc.cluster.local:80" },
] as const

describe("a Service by its Kubernetes type", () => {
  test.each([
    [{}, "ClusterIP", false],
    [{ type: "ClusterIP", clusterIP: "None" }, "Headless", false],
    [{ type: "NodePort" }, "NodePort", true],
    [{ type: "LoadBalancer" }, "LoadBalancer", true],
  ])("%p is %p, open to the internet: %p", (fields, label, out) => {
    const p = port({ addresses: [...inCluster], nodePort: out ? 30080 : undefined })
    const e = service(fields as Partial<Endpoint>, p)
    expect(typeLabel(e)).toBe(label)
    expect(outside(e)).toBe(out)
  })
})

describe("the address rows of a port", () => {
  test("each kind with an address is a row, in the kinds' order", () => {
    const p = port({
      nodePort: 30080,
      addresses: [
        { kind: "loadBalancer", address: "203.0.113.10:80" },
        ...inCluster,
        { kind: "node", address: "198.51.100.5:30080" },
      ],
    })
    expect(addressRows(service({ type: "LoadBalancer" }, p), p)).toEqual([
      { kind: "project", addresses: ["web:80"], pending: undefined },
      { kind: "cluster", addresses: ["web.shop.svc.cluster.local:80"], pending: undefined },
      { kind: "node", addresses: ["198.51.100.5:30080"], pending: undefined },
      { kind: "loadBalancer", addresses: ["203.0.113.10:80"], pending: undefined },
    ])
  })

  test("a node port without node addresses and a load balancer without one stand in", () => {
    const p = port({ nodePort: 30080, addresses: [...inCluster] })
    const rows = addressRows(service({ type: "LoadBalancer" }, p), p)
    expect(rows.slice(2)).toEqual([
      { kind: "node", addresses: [], pending: { message: "addr.nodePortOnly", port: 30080 } },
      { kind: "loadBalancer", addresses: [], pending: { message: "ports.waiting", port: 80 } },
    ])
  })
})

test("only a port with a node port needs the nodes", () => {
  expect(hasNodePort([service({})])).toBe(false)
  expect(hasNodePort([service({}), service({ type: "NodePort" }, port({ nodePort: 30080 }))])).toBe(
    true,
  )
})
