import { describe, expect, test } from "bun:test"

import type { Target } from "@/model/connect"
import {
  pointServices,
  restoreChanges,
  restoredName,
  restoreKin,
  restorePoints,
} from "@/model/database-restore"
import type { Database } from "@/model/databases"
import type { Service } from "@/model/service"

type Body = { kind: string; metadata: { name: string; annotations?: object }; spec: any }

const bodies = (changes: ReturnType<typeof restoreChanges>) =>
  changes.map((c) => (c.op === "create" ? (c.object as unknown as Body) : undefined))

const run = (name: string, restoreFrom?: string, outcome = "succeeded") => ({
  name,
  provider: "x",
  outcome,
  restoreFrom,
  finished: "2026-10-22T03:01:00Z",
  targets: [],
})

const base = (patch: Partial<Database>): Database =>
  ({
    name: "shop",
    members: 1,
    storage: "5Gi",
    storageClass: "fast",
    pods: [],
    claims: [],
    state: "ready",
    connection: {
      host: "shop-rw.demo.svc",
      port: 5432,
      secret: "shop-app",
      passwordKey: "password",
    },
    app: { username: "app", database: "app", secret: "shop-app", passwordKey: "password" },
    backups: { schedules: [], runs: [], ready: true },
    ...patch,
  }) as Database

const point = { backup: "shop-1", time: "2026-10-22T03:01:00Z", from: "20261022T030000" }

describe("restore points", () => {
  test("are the succeeded runs that name what to restore; Dragonfly's is its newest snapshot", () => {
    const pg = base({
      engine: "postgresql",
      backups: {
        schedules: [],
        ready: true,
        runs: [run("a", "id-a"), run("b", undefined), run("c", "id-c", "failed")],
      } as Database["backups"],
    })
    expect(restorePoints(pg).map((p) => p.from)).toEqual(["id-a"])
    const cache = base({
      engine: "dragonfly",
      backups: { schedules: [], runs: [], ready: false, store: { name: "x", inUse: true } },
    })
    expect(restorePoints(cache)).toEqual([{ backup: "latest" }])
  })

  test("the new name is the original's with the date, cut to fit", () => {
    const now = new Date(2026, 9, 3)
    expect(restoredName("shop", now)).toBe("shop-restored-1003")
    expect(restoredName("a-very-long-database-name", now)).toBe("a-very-long-data-restored-1003")
  })
})

describe("restoring into a new database", () => {
  test("PostgreSQL recovers from the backup's ID through the plugin, without the original's backups", () => {
    const pg = base({
      engine: "postgresql",
      apiVersion: "postgresql.cnpg.io/v1",
      kind: "Cluster",
      backups: {
        schedules: [],
        runs: [],
        ready: true,
        store: { name: "shop-backups", inUse: true },
      },
      object: {
        spec: { instances: 2, plugins: [{ name: "barman-cloud.cloudnative-pg.io" }] },
      } as unknown as Database["object"],
    })
    const [cluster] = bodies(restoreChanges(pg, point, "copy", "demo"))
    expect(cluster?.metadata).toEqual({
      name: "copy",
      annotations: {
        "zeabur.com/restored-from": "shop",
        "zeabur.com/restored-backup": "shop-1",
        "zeabur.com/restored-time": "2026-10-22T03:01:00Z",
      },
    })
    expect(cluster?.spec.instances).toBe(2)
    expect(cluster?.spec.plugins).toBeUndefined()
    expect(cluster?.spec.bootstrap.recovery).toEqual({
      source: "shop",
      database: "app",
      owner: "app",
      recoveryTarget: { backupID: "20261022T030000", targetImmediate: true },
    })
    expect(cluster?.spec.externalClusters[0].plugin.parameters).toEqual({
      barmanObjectName: "shop-backups",
      serverName: "shop",
    })
  })

  test("MongoDB keeps the original's users Secret and restores the destination into the copy", () => {
    const s3 = { bucket: "b", prefix: "shop", credentialsSecret: "shop-backups-s3" }
    const mongo = base({
      engine: "mongodb",
      apiVersion: "psmdb.percona.com/v1",
      kind: "PerconaServerMongoDB",
      backups: { schedules: [], runs: [], ready: true, store: { name: "backups", inUse: true } },
      object: {
        metadata: { finalizers: ["percona.com/delete-psmdb-pvc"] },
        spec: { backup: { enabled: true, storages: { backups: { s3 } }, tasks: [{ name: "x" }] } },
      } as unknown as Database["object"],
    })
    const [cluster, restore] = bodies(
      restoreChanges(mongo, { ...point, from: "s3://b/shop/2026" }, "copy", "demo"),
    )
    expect(cluster?.spec.backup.tasks).toEqual([])
    expect(cluster?.spec.secrets.users).toBe("shop-secrets")
    expect(restore).toMatchObject({
      kind: "PerconaServerMongoDBRestore",
      spec: {
        clusterName: "copy",
        backupSource: { type: "logical", destination: "s3://b/shop/2026", s3 },
      },
    })
  })

  test("a copy of a MongoDB 8 keeps rseq in glibc", () => {
    const mongo = base({
      engine: "mongodb",
      apiVersion: "psmdb.percona.com/v1",
      kind: "PerconaServerMongoDB",
      backups: { schedules: [], runs: [], ready: true, store: { name: "backups", inUse: true } },
      object: {
        metadata: {},
        spec: {
          image: "percona/percona-server-mongodb:8.0.32-14",
          replsets: [{ name: "rs0", size: 1 }],
          backup: { enabled: true, storages: { backups: { s3: {} } } },
        },
      } as unknown as Database["object"],
    })
    const [cluster] = bodies(restoreChanges(mongo, point, "copy", "demo"))
    expect(cluster?.spec.replsets[0].env).toEqual([
      { name: "GLIBC_TUNABLES", value: "glibc.pthread.rseq=1" },
    ])
  })

  test("MySQL restores to the backup's time, reading the same bucket and the same app Secret", () => {
    const mysql = base({
      engine: "mysql",
      apiVersion: "moco.cybozu.com/v1beta2",
      kind: "MySQLCluster",
      backups: {
        schedules: [],
        runs: [],
        ready: true,
        store: {
          name: "shop-backups",
          destination: "s3://b/moco/demo/shop",
          endpoint: "https://s3.test",
          secret: "shop-backups-s3",
          inUse: true,
        },
      },
      object: {
        metadata: {
          annotations: { "zeabur.com/app-database": "app", "zeabur.com/app-secret": "shop-app" },
        },
        spec: { replicas: 3, backupPolicyName: "shop-backups" },
      } as unknown as Database["object"],
    })
    const [cluster] = bodies(
      restoreChanges(mysql, { ...point, from: "2026-10-22T03:00:00Z" }, "copy", "demo"),
    )
    expect(cluster?.metadata.annotations).toMatchObject({ "zeabur.com/app-secret": "shop-app" })
    expect(cluster?.spec.backupPolicyName).toBeUndefined()
    expect(cluster?.spec.restore).toMatchObject({
      sourceName: "shop",
      sourceNamespace: "demo",
      restorePoint: "2026-10-22T03:00:00Z",
      jobConfig: {
        bucketConfig: { bucketName: "b", endpointURL: "https://s3.test" },
        envFrom: [{ secretRef: { name: "shop-backups-s3" } }],
      },
    })
  })

  test("MariaDB starts from the scheduled Backup, with an app password of its own", () => {
    const maria = base({
      engine: "mariadb",
      apiVersion: "k8s.mariadb.com/v1alpha1",
      kind: "MariaDB",
      backups: {
        schedules: [],
        runs: [],
        ready: true,
        store: { name: "shop-scheduled", inUse: true },
      },
      object: {
        spec: {
          rootPasswordSecretKeyRef: { name: "shop-root", key: "password" },
          passwordSecretKeyRef: { name: "shop-app", key: "password" },
        },
      } as unknown as Database["object"],
    })
    const [secret, server] = bodies(
      restoreChanges(maria, { ...point, from: "2026-10-22T03:01:00Z" }, "copy", "demo", () => "pw"),
    )
    expect(secret?.metadata.name).toBe("copy-app")
    expect(server?.spec.rootPasswordSecretKeyRef.name).toBe("shop-root")
    expect(server?.spec.passwordSecretKeyRef.name).toBe("copy-app")
    expect(server?.spec.bootstrapFrom).toEqual({
      backupRef: { name: "shop-scheduled" },
      targetRecoveryTime: "2026-10-22T03:01:00Z",
    })
  })

  test("Dragonfly copies the newest snapshot into a folder of its own before it starts", () => {
    const cache = base({
      engine: "dragonfly",
      apiVersion: "dragonflydb.io/v1alpha1",
      kind: "Dragonfly",
      backups: {
        schedules: [],
        runs: [],
        ready: false,
        store: {
          name: "shop",
          destination: "s3://b/shop",
          endpoint: "https://s3.test",
          inUse: true,
        },
      },
      object: {
        spec: {
          authentication: { passwordFromSecret: { name: "shop-auth", key: "password" } },
          snapshot: { dir: "s3://b/shop", cron: "0 3 * * *" },
          env: [
            { name: "AWS_ACCESS_KEY_ID", valueFrom: {} },
            { name: "OTHER", value: "x" },
          ],
        },
      } as unknown as Database["object"],
    })
    const [, store] = bodies(
      restoreChanges(cache, { backup: "latest" }, "copy", "demo", () => "pw"),
    )
    expect(store?.spec.snapshot.dir).toBe("s3://b/copy")
    expect(store?.spec.authentication.passwordFromSecret.name).toBe("copy-auth")
    const copy = store?.spec.initContainers[0]
    expect(copy.env.map((e: { name: string }) => e.name)).toEqual([
      "FROM",
      "TO",
      "ENDPOINT",
      "HOME",
      "AWS_ACCESS_KEY_ID",
    ])
    expect(copy.env[0].value).toBe("s3://b/shop")
  })
})

describe("moving services between the original and the restored database", () => {
  type Env = Service["containers"][number]["env"][number]
  const literal = (name: string, value: string) =>
    ({ name, value, source: { kind: "Literal" } }) as Env
  const fromSecret = (name: string, secret: string, key: string) =>
    ({ name, source: { kind: "Secret", name: secret, key } }) as Env
  const target = (name: string, env: Env[]): Target => ({
    kind: "Deployment",
    name,
    containers: [{ name, env }] as Target["containers"],
  })

  const mongo = (name: string, secret: string) =>
    base({
      engine: "mongodb",
      name,
      connection: {
        host: `${name}-rs0.demo.svc`,
        port: 27017,
        secret: `${name}-secrets`,
        passwordKey: "p",
      },
      app: { username: "app", database: "app", secret, passwordKey: "app" },
    })

  test("a Secret variable reads the copy's Secret, and a URL its host; others are left alone", () => {
    const from = mongo("shop", "shop-custom-user-secret")
    const to = mongo("copy", "copy-custom-user-secret")
    const api = target("api", [
      fromSecret("MONGOPASSWORD", "shop-custom-user-secret", "app"),
      literal("MONGO_URL", "mongodb://app:$(MONGOPASSWORD)@shop-rs0.demo.svc:27017/app"),
      literal("PORT", "8080"),
    ])
    expect(pointServices(from, to, [api, target("idle", [literal("X", "y")])])).toEqual([
      {
        op: "env",
        workload: { kind: "Deployment", name: "api" },
        changes: [
          {
            op: "set",
            name: "MONGOPASSWORD",
            valueFrom: { secretName: "copy-custom-user-secret", key: "app" },
          },
          {
            op: "set",
            name: "MONGO_URL",
            value: "mongodb://app:$(MONGOPASSWORD)@copy-rs0.demo.svc:27017/app",
          },
        ],
      },
    ])
  })

  test("the copy knows its source, and the source its copies", () => {
    const shop = mongo("shop", "a")
    const copy = { ...mongo("copy", "b"), restored: { from: "shop" } }
    expect(restoreKin(copy, [shop, copy]).source).toBe(shop)
    expect(restoreKin(shop, [shop, copy]).copies).toEqual([copy])
  })
})
