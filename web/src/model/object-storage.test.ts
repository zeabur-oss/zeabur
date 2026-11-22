import { describe, expect, test } from "bun:test"

import {
  bucketProblem,
  createObjectStorage,
  deleteObjectStorage,
  growObjectStorage,
  newObjectStorage,
  type ObjectStorage,
} from "@/model/object-storage"

type Made = { kind: string; metadata: { name: string }; spec: Record<string, any> }
const made = (changes: ReturnType<typeof createObjectStorage>) =>
  changes.flatMap((c) => (c.op === "create" ? [c.object as unknown as Made] : []))

describe("object storage", () => {
  test("a new store is a Seaweed, one IAM user with full access, and its buckets", () => {
    const objects = made(createObjectStorage({ ...newObjectStorage(), buckets: ["uploads"] }))
    expect(objects.map((o) => `${o.kind}/${o.metadata.name}`)).toEqual([
      "Seaweed/files",
      "S3Identity/files-app",
      "S3Credentials/files-app",
      "S3Policy/files-app",
      "S3PolicyBinding/files-app",
      "Bucket/files-uploads",
    ])
    const seaweed = objects[0]!.spec
    for (const part of ["master", "volume", "filer", "s3"])
      expect(seaweed[part].podSecurityContext.runAsNonRoot).toBe(true)
    expect(seaweed.master.persistence.enabled).toBe(true)
    expect(objects[5]!.spec).toMatchObject({ name: "uploads", clusterRef: { name: "files" } })
  })

  test.each([
    ["3Gi", 3 * 1024],
    ["10Gi", 10 * 1024],
    ["10Ti", 10 * 1024 * 1024],
  ])("a %s store has volumes for many buckets within its disk", (storage, mib) => {
    const [seaweed] = made(createObjectStorage({ ...newObjectStorage(), storage }))
    const size: number = seaweed!.spec.master.volumeSizeLimitMB
    const count: number = seaweed!.spec.volume.maxVolumeCounts
    expect(count).toBeGreaterThanOrEqual(7 * 10)
    expect(size).toBeLessThanOrEqual(30_000)
    expect(size * count).toBeLessThanOrEqual(mib)
    expect(size * (count + 1)).toBeGreaterThan(mib)
  })

  test("deleting goes through the buckets and IAM objects before the Seaweed", () => {
    const store = {
      name: "files",
      buckets: [{ name: "files-uploads" }],
      iam: [
        { kind: "S3Identity", name: "files-app" },
        { kind: "S3Credentials", name: "files-app" },
      ],
    } as ObjectStorage
    expect(deleteObjectStorage(store).map((c) => c.op === "delete" && c.resource.resource)).toEqual(
      ["buckets", "s3identities", "s3credentials", "seaweeds"],
    )
  })

  test("growing the disks changes only the spec, keeping its other requests", () => {
    const store = {
      name: "files",
      object: { spec: { volume: { requests: { cpu: "1", storage: "10Gi" } } } },
    } as unknown as ObjectStorage
    const [grow] = growObjectStorage(store, " 20Gi ")
    expect(grow).toMatchObject({ op: "edit", name: "files", specOnly: true })
    const yaml = grow?.op === "edit" ? JSON.parse(grow.yaml) : undefined
    expect(yaml.spec.volume.requests).toEqual({ cpu: "1", storage: "20Gi" })
    const { volumeSizeLimitMB: size } = yaml.spec.master
    expect(size * yaml.spec.volume.maxVolumeCounts).toBeGreaterThan(10 * 1024)
  })

  test("bucket names follow the server's bucket rule", () => {
    expect(bucketProblem("uploads", [])).toBeUndefined()
    expect(bucketProblem("", [])).toBe("empty")
    expect(bucketProblem("Up", [])).toBe("invalid")
    expect(bucketProblem("ab", [])).toBe("invalid")
    expect(bucketProblem("uploads", ["uploads"])).toBe("taken")
  })
})
