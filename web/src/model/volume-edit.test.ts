import { describe, expect, test } from "bun:test"

import type { Schemas } from "@/api/client"
import {
  bytesOf,
  changesOf,
  editedCount,
  nameFromPath,
  problemsOf,
  rowState,
  type VolumeRow,
} from "@/model/volume-edit"

const workload = { kind: "Deployment", name: "api" } as Schemas["WorkloadRef"]

const existing = (mountPath: string, size: string, claim = `api-${nameFromPath(mountPath)}`) =>
  ({
    id: mountPath,
    mountPath,
    size,
    readOnly: false,
    was: { mountPath, size, readOnly: false, claim },
    removed: false,
    name: null,
    claimName: null,
    accessMode: "ReadWriteOnce",
  }) satisfies VolumeRow

const added = (id: string, mountPath: string, size = "1Gi"): VolumeRow => ({
  id,
  mountPath,
  size,
  readOnly: false,
  removed: false,
  name: null,
  claimName: null,
  accessMode: "ReadWriteOnce",
})

describe("volume rows", () => {
  test("a name follows the mount path's last part", () => {
    expect(nameFromPath("/var/lib/data")).toBe("data")
    expect(nameFromPath("/srv/My_Files/")).toBe("my-files")
    expect(nameFromPath("/")).toBe("data")
  })

  test("sizes compare across units", () => {
    expect(bytesOf("1Gi")).toBe(2 ** 30)
    expect(bytesOf("1500M")).toBe(1.5e9)
    expect(bytesOf("1 Gi")).toBeUndefined()
    expect(bytesOf("big")).toBeUndefined()
  })

  test("a row is the same until a field moves from what the cluster has", () => {
    const row = existing("/data", "1Gi")
    expect(rowState(row)).toBe("same")
    expect(rowState({ ...row, size: "2Gi" })).toBe("changed")
    expect(rowState({ ...row, removed: true })).toBe("removed")
    expect(rowState(added("n", "/cache"))).toBe("added")
  })

  test("an empty added row counts for nothing", () => {
    expect(editedCount([existing("/data", "1Gi"), added("n", "")])).toBe(0)
    expect(changesOf([added("n", "  ")], workload)).toEqual([])
    expect(problemsOf([added("n", "")]).size).toBe(0)
  })

  test("problems name the first thing to fix in each row", () => {
    const problems = problemsOf(
      [
        { ...existing("/data", "2Gi"), size: "1Gi" },
        added("a", "cache"),
        added("b", "/logs", "lots"),
        added("c", "/srv/x"),
        added("d", "/opt/x"),
        added("e", "/tmp"),
      ],
      new Set(["/tmp"]),
    )
    expect(Object.fromEntries(problems)).toEqual({
      "/data": "shrinks",
      a: "badPath",
      b: "badSize",
      c: "nameTaken",
      d: "nameTaken",
      e: "pathTaken",
    })
  })

  test("a removed path is free for another row", () => {
    const rows = [{ ...existing("/data", "1Gi"), removed: true }, added("n", "/data")]
    expect(problemsOf(rows).size).toBe(0)
  })

  test("one plan removes, then moves, then grows, then adds", () => {
    const rows = [
      added("n", "/cache", "2Gi"),
      { ...existing("/data", "1Gi"), size: "5Gi", mountPath: "/var/data" },
      { ...existing("/old", "1Gi"), removed: true },
      { ...existing("/conf", "1Gi"), readOnly: true },
    ]
    expect(changesOf(rows, workload)).toEqual([
      { op: "removeVolume", workload, container: undefined, mountPath: "/old" },
      {
        op: "editMount",
        workload,
        container: undefined,
        mountPath: "/data",
        newMountPath: "/var/data",
        readOnly: undefined,
      },
      {
        op: "editMount",
        workload,
        container: undefined,
        mountPath: "/conf",
        newMountPath: undefined,
        readOnly: true,
      },
      { op: "expandVolume", claim: "api-data", size: "5Gi" },
      {
        op: "addVolume",
        workload,
        container: undefined,
        volume: {
          name: "cache",
          mountPath: "/cache",
          size: "2Gi",
          claimName: undefined,
          accessMode: undefined,
        },
      },
    ])
  })

  test("a new volume asks for the storage class chosen for it", () => {
    const [change] = changesOf([{ ...added("n", "/files"), storageClass: "longhorn" }], workload)
    expect(change).toMatchObject({ op: "addVolume", volume: { storageClass: "longhorn" } })
  })
})
