import { describe, expect, test } from "bun:test"

import {
  backupFates,
  dataFate,
  deletingCommands,
  projectDeletionOf,
  recentBackup,
} from "@/model/project-deletion"

const now = Date.parse("2026-10-26T12:00:00Z")

describe("project deletion", () => {
  test("a volume's data follows its reclaim policy, and an unread policy is not guessed", () => {
    expect(dataFate({ claim: "data", reclaimPolicy: "Delete" })).toBe("deleted")
    expect(dataFate({ claim: "data", reclaimPolicy: "Retain", volume: "pv-1" })).toBe("kept")
    expect(dataFate({ claim: "data" })).toBe("unknown")
  })

  test("backups kept in the project go with it, grouped by what keeps them", () => {
    const fates = backupFates([
      { kind: "database", name: "pg", fate: "goes", keptIn: "files" },
      { kind: "volume", name: "uploads", fate: "goes", keptIn: "files" },
      { kind: "database", name: "orders", fate: "stays" },
      { kind: "project", name: "shop", fate: "unknown" },
    ])
    expect(fates.going.map((g) => [g.keptIn, g.backups.map((b) => b.name)])).toEqual([
      ["files", ["pg", "uploads"]],
    ])
    expect(fates.staying.map((b) => b.name)).toEqual(["orders"])
    expect(fates.unknown.map((b) => b.name)).toEqual(["shop"])
    expect(backupFates([])).toEqual({ going: [], unknown: [], staying: [] })
  })

  test("only a backup that succeeded within a day is recent", () => {
    const run = (outcome: "succeeded" | "failed", finished: string) => ({
      name: "b",
      provider: "k8up",
      outcome,
      finishedAt: finished,
      startedAt: finished,
      targets: [],
    })
    expect(recentBackup(run("succeeded", "2026-10-26T02:00:00Z"), now)).toBe(true)
    expect(recentBackup(run("succeeded", "2026-10-24T12:00:00Z"), now)).toBe(false)
    expect(recentBackup(run("failed", "2026-10-26T11:00:00Z"), now)).toBe(false)
    expect(recentBackup(undefined, now)).toBe(false)
  })

  test("the deletion is found among a plan's warnings", () => {
    const deletion = {
      code: "deletesProject" as const,
      namespace: "shop",
      services: [],
      databases: [],
      volumes: [],
      toolsNamespace: "zeabur-terminals",
      toolsPods: [],
      backups: [],
    }
    expect(
      projectDeletionOf([
        { code: "configNotRolledBack", workload: { kind: "Deployment", name: "web" } },
        deletion,
      ]),
    ).toBe(deletion)
    expect(
      projectDeletionOf([
        { code: "configNotRolledBack", workload: { kind: "Deployment", name: "web" } },
      ]),
    ).toBeUndefined()
  })

  test("the commands show the namespace, then each held object by resource and group", () => {
    const commands = deletingCommands("shop", [
      {
        group: "postgresql.cnpg.io",
        resource: "clusters",
        kind: "Cluster",
        name: "db",
        finalizers: ["cnpg.io/cleanup"],
      },
      {
        group: "",
        resource: "persistentvolumeclaims",
        kind: "PersistentVolumeClaim",
        name: "data",
        finalizers: ["kubernetes.io/pvc-protection"],
      },
    ])
    expect(commands.split("\n")).toEqual([
      "kubectl get namespace shop -o yaml",
      "kubectl get clusters.postgresql.cnpg.io db -n shop -o yaml",
      "kubectl get persistentvolumeclaims data -n shop -o yaml",
      "",
    ])
  })
})
