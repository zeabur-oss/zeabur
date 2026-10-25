import { describe, expect, test } from "bun:test"

import {
  restoreInPlace,
  restoreToNewVolume,
  swapToVolume,
  backUpVolumeNow,
  backupUser,
  newVolumeTarget,
  scheduleOf,
  scheduleVolumeBackups,
  volumeTargetProblems,
} from "@/model/volume-backups"

const target = {
  ...newVolumeTarget(),
  endpoint: "https://s3.test/",
  bucket: "backups",
  accessKey: "id",
  secretKey: "key",
  region: "eu",
}

describe("volume backups", () => {
  test("keys from an S3 destination are copied by the plan", () => {
    const [secret] = scheduleVolumeBackups(
      "shop",
      "uploads",
      { ...target, destination: "r2" },
      { root: true },
      () => "pw",
    )
    expect(secret).toMatchObject({
      op: "copyS3Keys",
      destination: "r2",
      accessKeyId: "AWS_ACCESS_KEY_ID",
      object: { metadata: { name: "uploads-backup" }, stringData: { RESTIC_PASSWORD: "pw" } },
    })
  })

  test("a schedule backs up the labelled claim alone, to its own folder, and prunes by the retention", () => {
    const changes = scheduleVolumeBackups("shop", "uploads", target, { root: true }, () => "pw")
    expect(changes[0]).toMatchObject({
      op: "create",
      object: {
        kind: "Secret",
        metadata: { name: "uploads-backup" },
        stringData: {
          AWS_ACCESS_KEY_ID: "id",
          AWS_SECRET_ACCESS_KEY: "key",
          RESTIC_PASSWORD: "pw",
          AWS_DEFAULT_REGION: "eu",
        },
      },
    })
    expect(changes[1]).toEqual({
      op: "labelVolume",
      claim: "uploads",
      labels: { "zeabur.com/backup-volume": "uploads" },
      annotations: { "k8up.io/backup": "true" },
    })
    expect(changes[2]).toMatchObject({
      object: {
        apiVersion: "k8up.io/v1",
        kind: "Schedule",
        spec: {
          backend: {
            s3: { endpoint: "https://s3.test", bucket: "backups/shop/uploads" },
            envFrom: [{ secretRef: { name: "uploads-backup" } }],
          },
          backup: {
            schedule: "0 3 * * *",
            labelSelectors: [{ matchLabels: { "zeabur.com/backup-volume": "uploads" } }],
          },
          prune: { retention: { keepLast: 3, keepDaily: 7, keepWeekly: 4 } },
          podSecurityContext: { runAsUser: 0 },
        },
      },
    })
  })

  test("backups run as the service's user, and as root only when the service may", () => {
    expect(backupUser({ runAs: { user: 1000, fsGroup: 1000 }, podSecurity: "restricted" })).toEqual(
      { root: false, user: 1000, fsGroup: 1000 },
    )
    expect(backupUser({ podSecurity: "restricted" })).toEqual({ root: false })
    expect(backupUser({ podSecurity: "baseline" })).toEqual({ root: true })
    expect(backupUser({ runAs: { user: 0 }, podSecurity: "baseline" })).toEqual({ root: true })

    const changes = scheduleVolumeBackups("shop", "uploads", target, { root: false, user: 1000 })
    const kinds = changes.map((c) =>
      c.op === "create" ? String((c.object as { kind?: string } | undefined)?.kind) : c.op,
    )
    expect(kinds).toEqual(["Secret", "PodConfig", "labelVolume", "Schedule"])
    expect(changes[1]).toMatchObject({
      object: {
        metadata: { name: "uploads-backup" },
        spec: {
          template: {
            spec: {
              securityContext: {
                runAsNonRoot: true,
                runAsUser: 1000,
                seccompProfile: { type: "RuntimeDefault" },
              },
              containers: [
                {
                  securityContext: {
                    allowPrivilegeEscalation: false,
                    capabilities: { drop: ["ALL"] },
                  },
                },
              ],
            },
          },
        },
      },
    })
    expect(changes[3]).toMatchObject({
      object: { spec: { podConfigRef: { name: "uploads-backup" } } },
    })
  })

  test("retention counts are whole and bounded", () => {
    expect(volumeTargetProblems(target)).toEqual([])
    expect(volumeTargetProblems({ ...target, keepLast: -1 })).toEqual(["keep"])
    expect(volumeTargetProblems({ ...target, keepWeekly: 1.5 })).toEqual(["keep"])
  })

  test("backing up now names the schedule's one-off Backup", () => {
    const schedule = {
      name: "shop/uploads-backup",
      provider: "k8up",
      cron: "0 3 * * *",
      paused: false,
      targets: [{ kind: "volume" as const, name: "uploads" }],
      runNow: { apiVersion: "k8up.io/v1", kind: "Backup", spec: { tags: [] } } as unknown as Record<
        string,
        never
      >,
    }
    const data = { schedules: [schedule] } as unknown as Parameters<typeof scheduleOf>[0]
    expect(scheduleOf(data, "uploads")).toBe(schedule)
    expect(backUpVolumeNow(schedule, "uploads", new Date("2026-10-24T08:09:10Z"))).toEqual([
      {
        op: "create",
        object: {
          apiVersion: "k8up.io/v1",
          kind: "Backup",
          spec: { tags: [] },
          metadata: { name: "uploads-20261024-080910" },
        } as unknown as Record<string, never>,
      },
    ])
  })
})

describe("restoring a volume from a backup point", () => {
  const template = {
    apiVersion: "k8up.io/v1",
    kind: "Restore",
    metadata: { labels: { "zeabur.com/backup-volume": "data" } },
    spec: {
      backend: { s3: { bucket: "backups/shop/data" } },
      podConfigRef: { name: "data-backup" },
    },
  }
  const now = new Date("2026-10-25T04:05:06Z")
  const objects = (changes: ReturnType<typeof restoreToNewVolume>) =>
    changes.map((c) => (c.op === "create" ? (c.object as any) : c))

  test("goes into a new claim of the same size and class, with the backup's backend and user", () => {
    const [claim, restore] = objects(
      restoreToNewVolume(
        template,
        "abc123",
        "data",
        "data-restored-1025",
        {
          size: "5Gi",
          storageClass: "fast",
          accessModes: ["ReadWriteOnce"],
        },
        now,
      ),
    )
    expect(claim.kind).toBe("PersistentVolumeClaim")
    expect(claim.metadata.name).toBe("data-restored-1025")
    expect(claim.spec).toEqual({
      accessModes: ["ReadWriteOnce"],
      storageClassName: "fast",
      resources: { requests: { storage: "5Gi" } },
    })
    expect(restore.metadata).toEqual({
      labels: { "zeabur.com/backup-volume": "data" },
      name: "data-restore-20261025-040506",
    })
    expect(restore.spec).toEqual({
      backend: { s3: { bucket: "backups/shop/data" } },
      podConfigRef: { name: "data-backup" },
      snapshot: "abc123",
      restoreMethod: { folder: { claimName: "data-restored-1025" } },
    })
  })

  test("in place, stops the service first and deletes what the backup does not have", () => {
    const workload = { kind: "StatefulSet" as const, name: "db" }
    const [scale, restore] = objects(restoreInPlace(template, "abc123", "data", workload, 2, now))
    expect(scale).toEqual({ op: "scale", workload, replicas: 0 })
    expect(restore.spec.delete).toBe(true)
    expect(restore.spec.restoreMethod).toEqual({ folder: { claimName: "data" } })
    expect(restore.metadata.annotations).toEqual({ "zeabur.com/replicas": "2" })
  })

  test("the service switches claims in one change", () => {
    expect(swapToVolume({ kind: "Deployment", name: "web" }, "data", "data-restored-1025")).toEqual(
      [
        {
          op: "swapClaim",
          workload: { kind: "Deployment", name: "web" },
          from: "data",
          to: "data-restored-1025",
        },
      ],
    )
  })
})
