import { describe, expect, test } from "bun:test"

import {
  canGrow,
  chosenClass,
  classNamed,
  offeredClasses,
  placementOf,
  type ProjectStorage,
  type StorageClass,
} from "@/model/storage"

const origin = (name: string) => ({ apiVersion: "storage.k8s.io/v1", kind: "StorageClass", name })

const localPath: StorageClass = {
  origin: origin("local-path"),
  name: "local-path",
  provisioner: "rancher.io/local-path",
  kind: "local",
  default: true,
  allowExpansion: false,
  reclaimPolicy: "Delete",
  bindingMode: "WaitForFirstConsumer",
}

const longhorn: StorageClass = {
  origin: origin("longhorn"),
  name: "longhorn",
  provisioner: "driver.longhorn.io",
  kind: "replicated",
  default: false,
  allowExpansion: true,
  reclaimPolicy: "Delete",
  bindingMode: "Immediate",
}

const storage: ProjectStorage = {
  id: "projects/shop/storage",
  classes: [longhorn, localPath],
  claims: [],
  unreadable: [],
}

const refused: ProjectStorage = {
  ...storage,
  classes: [],
  unreadable: [
    {
      resource: "storageclasses.storage.k8s.io",
      kubernetes: { code: 403, reason: "Forbidden", message: "forbidden" },
    },
  ],
}

const claim = { origin: origin("data"), claim: "data", storageClass: "local-path" }

describe("classes", () => {
  test("a claim that names no class is on the default", () => {
    expect(classNamed(storage, undefined)?.name).toBe("local-path")
    expect(classNamed(storage, "longhorn")?.name).toBe("longhorn")
  })

  test("growth follows the claim's own class and is unknown when classes are refused", () => {
    expect(canGrow(storage, "local-path")).toBe(false)
    expect(canGrow(storage, "longhorn")).toBe(true)
    expect(canGrow(storage, null)).toBe(false)
    expect(canGrow(refused, "longhorn")).toBeUndefined()
  })

  test("the picker offers the default first and sends no class for it", () => {
    expect(offeredClasses(storage).map((c) => c.name)).toEqual(["local-path", "longhorn"])
    expect(chosenClass(storage, "local-path")).toBeUndefined()
    expect(chosenClass(storage, "longhorn")).toBe("longhorn")
    expect(chosenClass(refused, undefined)).toBeUndefined()
  })
})

describe("placement", () => {
  test("a pinned volume names its node, a Longhorn one moves", () => {
    expect(placementOf({ ...claim, pinned: true, node: "vps-3" }, localPath)).toEqual({
      kind: "node",
      node: "vps-3",
    })
    expect(placementOf({ ...claim, pinned: false }, longhorn)).toEqual({ kind: "moves" })
  })

  test("without its volume, a local claim is placed by its replica or by first use", () => {
    expect(placementOf({ ...claim, volume: "pvc-1" }, localPath, "vps-2")).toEqual({
      kind: "node",
      node: "vps-2",
    })
    expect(placementOf({ ...claim, volume: "pvc-1" }, localPath)).toEqual({ kind: "pinned" })
    expect(placementOf(claim, localPath)).toEqual({ kind: "firstUse" })
    expect(placementOf(claim, undefined)).toBeUndefined()
  })
})
