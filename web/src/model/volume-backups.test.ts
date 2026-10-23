import { describe, expect, test } from "bun:test"

import {
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
