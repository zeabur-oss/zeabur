import { describe, expect, test } from "bun:test"

import type { Schemas } from "@/api/client"
import { groupIncidents } from "@/model/incidents"
import {
  byNodeSeverity,
  diskShare,
  fillHealth,
  groupIssues,
  isProblem,
  MEMORY,
  nodeIssues,
  worstNode,
} from "@/model/node-health"
import { missingReason, volumeFill } from "@/model/volume-use"

const GiB = 2 ** 30

const node = (name: string, more: Partial<Schemas["NodeView"]> = {}): Schemas["NodeView"] => ({
  id: `nodes/${name}`,
  name,
  allocatable: { cpuMillicores: 2000, memoryBytes: 2 * GiB, pods: 110 },
  capacity: { cpuMillicores: 2000, memoryBytes: 2 * GiB, pods: 110 },
  architecture: "amd64",
  containerRuntime: "containerd://1.7",
  cordoned: false,
  kernelVersion: "6.1",
  kubeletVersion: "v1.31.4+k3s1",
  osImage: "Debian",
  origin: { kind: "Node", name },
  problems: [],
  ready: "True",
  roles: [],
  taints: [],
  ...more,
})

const disk = (capacity: number, available: number) => ({
  capacityBytes: capacity * GiB,
  availableBytes: available * GiB,
  usedBytes: (capacity - available) * GiB,
})

const memoryUse = (share: number) => ({
  cpuMillicores: 100,
  memoryBytes: Math.round(share * 2 * GiB),
})

describe("fill", () => {
  test("is ok below the warning, then warn, then fail", () => {
    expect(fillHealth(undefined, MEMORY)).toBe("ok")
    expect(fillHealth(0.84, MEMORY)).toBe("ok")
    expect(fillHealth(0.85, MEMORY)).toBe("warn")
    expect(fillHealth(0.95, MEMORY)).toBe("fail")
  })

  test("a disk counts what is no longer free, including what the filesystem keeps back", () => {
    expect(diskShare(disk(40, 5))).toBe(0.875)
    expect(diskShare(null)).toBeUndefined()
    expect(diskShare({ capacityBytes: 0, availableBytes: 0, usedBytes: 0 })).toBeUndefined()
  })
})

describe("a node's issues", () => {
  test("a healthy node has none", () => {
    expect(nodeIssues(node("a"))).toEqual([])
  })

  test("memory nearly full leads with use, not with what pods reserve", () => {
    const issues = nodeIssues(
      node("vps-1", {
        usage: memoryUse(0.91),
        requests: { cpuMillicores: 100, memoryBytes: 0.2 * 2 * GiB },
      }),
    )
    expect(issues.map((i) => [i.kind, i.health])).toEqual([["memoryFull", "warn"]])
    expect(issues[0]?.share).toBeCloseTo(0.91, 2)
  })

  test("a node that is down has only that", () => {
    const issues = nodeIssues(
      node("a", { ready: "Unknown", usage: memoryUse(0.99), cordoned: true }),
    )
    expect(issues.map((i) => i.kind)).toEqual(["notReady"])
  })

  test("every pressure is listed, worst first, and a pressure replaces its own fill", () => {
    const issues = nodeIssues(
      node("a", {
        problems: [
          { type: "PIDPressure", status: "True" },
          { type: "DiskPressure", status: "True" },
          { type: "MemoryPressure", status: "True" },
        ],
        usage: memoryUse(0.97),
        disk: disk(40, 2),
        cordoned: true,
        requests: { cpuMillicores: 1900, memoryBytes: 3 * GiB },
      }),
    )
    expect(issues.map((i) => i.kind)).toEqual([
      "memoryPressure",
      "diskPressure",
      "pidPressure",
      "cordoned",
      "reservedMemory",
      "reservedCpu",
    ])
    expect(issues.filter(isProblem).map((i) => i.kind)).toEqual([
      "memoryPressure",
      "diskPressure",
      "pidPressure",
    ])
  })

  test("a disk past the kubelet's eviction line fails", () => {
    expect(nodeIssues(node("a", { disk: disk(40, 3) }))[0]).toMatchObject({
      kind: "diskFull",
      health: "fail",
    })
    expect(nodeIssues(node("a", { disk: disk(40, 5) }))[0]).toMatchObject({
      kind: "diskFull",
      health: "warn",
    })
  })
})

describe("grouping by outcome", () => {
  test("no nodes, no groups", () => {
    expect(groupIssues([])).toEqual([])
  })

  test("the same outcome on many nodes is one group, fullest first", () => {
    const nodes = Array.from({ length: 25 }, (_, i) =>
      node(`n-${i}`, { usage: memoryUse(0.86 + i * 0.001) }),
    )
    const groups = groupIssues(nodes.flatMap(nodeIssues))
    expect(groups.map((g) => g.issues.map((i) => i.node))).toEqual([
      nodes.map((n) => n.name).reverse(),
    ])
  })

  test("a failing group comes before a larger warning one", () => {
    const groups = groupIssues(
      [
        node("a", { cordoned: true }),
        node("b", { cordoned: true }),
        node("c", { ready: "False" }),
      ].flatMap(nodeIssues),
    )
    expect(groups.map((g) => [g.kind, g.issues.length])).toEqual([
      ["notReady", 1],
      ["cordoned", 2],
    ])
  })
})

describe("nodes, worst first", () => {
  test("sorts down, then full, then fine by name", () => {
    const nodes = [
      node("c"),
      node("b", { usage: memoryUse(0.9) }),
      node("a"),
      node("z", { ready: "False" }),
    ]
    expect(nodes.toSorted(byNodeSeverity).map((n) => n.name)).toEqual(["z", "b", "a", "c"])
  })

  test("the totals name the worst node only when there are several and it is a problem", () => {
    expect(worstNode([])).toBeUndefined()
    expect(worstNode([node("only", { ready: "False" })])).toBeUndefined()
    expect(worstNode([node("a"), node("b", { cordoned: true })])).toBeUndefined()
    expect(worstNode([node("a"), node("b", { disk: disk(40, 3) }), node("c")])?.node.name).toBe("b")
  })

  test("a cordoned node says whether replicas are leaving, all are gone, or they stay", () => {
    const kind = (more: Partial<Schemas["NodeView"]>) =>
      nodeIssues(node("a", { cordoned: true, ...more })).map((i) => [i.kind, i.health])
    expect(kind({ movablePods: 3, leavingPods: 1 })).toEqual([["draining", "progress"]])
    expect(kind({ movablePods: 0, leavingPods: 0 })).toEqual([["maintenance", "idle"]])
    expect(kind({ movablePods: 0, leavingPods: 0, pinnedPods: 1 })).toEqual([
      ["maintenance", "idle"],
    ])
    expect(kind({ movablePods: 3, leavingPods: 0 })).toEqual([["cordoned", "warn"]])
    expect(kind({})).toEqual([["cordoned", "warn"]])
    expect(kind({ movablePods: 0 }).map(([k]) => k)).not.toContain("cordoned")
    expect(isProblem({ kind: "maintenance", node: "a", health: "idle" })).toBe(false)
  })
})

const volume = (claim: string, capacity: number, available: number): Schemas["VolumeUsage"] => ({
  claim,
  capacityBytes: capacity * GiB,
  availableBytes: available * GiB,
  usedBytes: (capacity - available) * GiB,
})

describe("a volume's fill", () => {
  test("nothing measured, nothing to say", () => {
    expect(volumeFill([], "data", "db")).toBeUndefined()
  })

  test("one claim: taken of capacity, amber from 85%", () => {
    const fill = volumeFill([volume("uploads", 10, 2.4)], "uploads", "web")
    expect(fill).toMatchObject({ replicas: 1, health: "ok" })
    expect(fill?.takenBytes).toBeCloseTo(7.6 * GiB)
    expect(volumeFill([volume("uploads", 10, 1.4)], "uploads", "web")?.health).toBe("warn")
  })

  test("a StatefulSet's replicas: the fullest claim counts, others' claims do not", () => {
    const items = [
      volume("data-db-0", 10, 8),
      volume("data-db-1", 10, 0.5),
      volume("data-db-2", 10, 6),
      volume("data-dbx-0", 10, 0),
    ]
    const fill = volumeFill(items, "data", "db")
    expect(fill).toMatchObject({ claim: "data-db-1", replicas: 3, health: "fail" })
  })

  test("a volume without a fill says why, never zero", () => {
    const fill = volumeFill([volume("uploads", 10, 2.4)], "uploads", "web")
    expect(missingReason("available", [fill])).toBeUndefined()
    expect(missingReason("available", [fill, undefined])).toBe("not-collected")
    expect(missingReason("not-configured", [undefined])).toBe("not-configured")
    expect(missingReason("unreadable", [undefined])).toBe("unreadable")
  })
})

const incident = (
  name: string,
  more: Partial<Schemas["Incident"]> = {},
  services: string[] = [],
): Schemas["Incident"] => ({
  id: `incidents/node/${name}`,
  kind: "nodeNotReady",
  cause: { key: "incident.nodeNotReady.cause", params: { node: name, status: "Unknown" } },
  why: { key: "incident.nodeNotReady.why" },
  fix: { key: "incident.nodeNotReady.fix", params: { node: name } },
  fault: { origin: "cluster", fixer: "clusterAdmin" },
  domains: [],
  evidence: [],
  projects: [],
  services: services.map((s) => {
    const [project = "", service = ""] = s.split("/")
    return { id: `${project}/Deployment/${service}`, project, kind: "Deployment", name: service }
  }) as Schemas["AffectedService"][],
  ...more,
})

describe("incidents", () => {
  test("none, no groups; one is its own group", () => {
    expect(groupIncidents([])).toEqual([])
    expect(groupIncidents([incident("a", {}, ["shop/web"])])).toMatchObject([
      { incidents: [{ id: "incidents/node/a" }], projects: 1 },
    ])
  })

  test("25 nodes not reporting are one group, with each service once", () => {
    const many = Array.from({ length: 25 }, (_, i) =>
      incident(`n-${i}`, {}, [`p-${i % 12}/web`, "shop/db"]),
    )
    const [group, ...rest] = groupIncidents(many)
    expect(rest).toEqual([])
    expect(group?.incidents).toHaveLength(25)
    expect(group?.services).toHaveLength(13)
    expect(group?.projects).toBe(13)
  })

  test("not ready and not reporting stay apart, and each component is its own", () => {
    const groups = groupIncidents([
      incident("a"),
      incident("b", {
        cause: { key: "incident.nodeNotReady.cause", params: { node: "b", status: "False" } },
      }),
      incident("c"),
      incident("x", { id: "incidents/component/cert-manager", kind: "componentDown" }),
      incident("y", { id: "incidents/component/cluster-dns", kind: "componentDown" }),
    ])
    expect(groups.map((g) => g.incidents.length)).toEqual([2, 1, 1, 1])
  })
})
