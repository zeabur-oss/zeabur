import { describe, expect, test } from "bun:test"

import type { Schemas } from "@/api/client"
import { groupIncidents } from "@/model/incidents"
import { diskShare, fillHealth } from "@/model/node-health"
import { missingReason, volumeFill } from "@/model/volume-use"

const GiB = 2 ** 30

const disk = (capacity: number, available: number) => ({
  capacityBytes: capacity * GiB,
  availableBytes: available * GiB,
  usedBytes: (capacity - available) * GiB,
})

const MEMORY = { warn: 0.85, fail: 0.95 }

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

const volume = (
  claim: string,
  capacity: number,
  available: number,
  health: Schemas.Health = "ok",
): Schemas.VolumeUsage => ({
  claim,
  health,
  capacityBytes: capacity * GiB,
  availableBytes: available * GiB,
  usedBytes: (capacity - available) * GiB,
})

describe("a volume's fill", () => {
  test("nothing measured, nothing to say", () => {
    expect(volumeFill([], "data", "db")).toBeUndefined()
  })

  test("one claim: taken of capacity, with the server's health", () => {
    const fill = volumeFill([volume("uploads", 10, 2.4)], "uploads", "web")
    expect(fill).toMatchObject({ replicas: 1, health: "ok" })
    expect(fill?.takenBytes).toBeCloseTo(7.6 * GiB)
    expect(volumeFill([volume("uploads", 10, 1.4, "warn")], "uploads", "web")?.health).toBe("warn")
  })

  test("a StatefulSet's replicas: the fullest claim counts, others' claims do not", () => {
    const items = [
      volume("data-db-0", 10, 8),
      volume("data-db-1", 10, 0.5, "fail"),
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
  more: Partial<Schemas.Incident> = {},
  services: string[] = [],
): Schemas.Incident => ({
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
  }) as Schemas.AffectedService[],
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
