import { describe, expect, test } from "bun:test"

import { actionOf, describe as describePlan } from "@/components/plan/describe"
import type { Translate } from "@/i18n"

import {
  backUpNow,
  backUpNowSummary,
  backupName,
  backupsSummary,
  canBackUpNow,
  commands,
  connectionUri,
  createDatabase,
  databaseAddress,
  DATABASE_LABEL,
  deleteDatabase,
  madeObjects,
  addPerThreadCaches,
  eventsOf,
  enableBackups,
  nameProblem,
  newBackupTarget,
  newDatabase,
  newPassword,
  presetOf,
  presetResources,
  reshapeDatabase,
  rseqNote,
  shellCommand,
  storageProblem,
  targetProblems,
  type Database,
} from "@/model/databases"

const postgres: Database = {
  apiVersion: "postgresql.cnpg.io/v1",
  kind: "Cluster",
  resource: "clusters",
  engine: "postgresql",
  provider: "cloudnative-pg",
  name: "pg",
  state: "ready",
  members: 1,
  ready: 1,
  primary: "pg-1",
  claims: ["pg-1"],
  pods: [],
  origin: {} as Database["origin"],
  connection: {
    host: "pg-rw.shop.svc",
    port: 5432,
    database: "app",
    username: "app",
    secret: "pg-app",
    usernameKey: "username",
    passwordKey: "password",
  },
  backups: { schedules: [], runs: [], ready: false },
  object: {
    apiVersion: "postgresql.cnpg.io/v1",
    kind: "Cluster",
    metadata: { name: "pg", namespace: "shop", finalizers: ["x"] },
    spec: { instances: 1, plugins: [{ name: "other" }] },
  } as unknown as Database["object"],
}

const mongo: Database = {
  ...postgres,
  apiVersion: "psmdb.percona.com/v1",
  kind: "PerconaServerMongoDB",
  resource: "perconaservermongodbs",
  engine: "mongodb",
  provider: "percona-server-mongodb",
  name: "mongo",
  primary: null,
  connection: {
    host: "mongo-rs0.shop.svc",
    port: 27017,
    database: "admin",
    secret: "mongo-secrets",
    usernameKey: "MONGODB_DATABASE_ADMIN_USER",
    passwordKey: "MONGODB_DATABASE_ADMIN_PASSWORD",
    options: "replicaSet=rs0",
  },
  object: {
    apiVersion: "psmdb.percona.com/v1",
    kind: "PerconaServerMongoDB",
    metadata: { name: "mongo" },
    spec: { replsets: [{ name: "rs0", size: 1 }], backup: { enabled: false, image: "pbm:1" } },
  } as unknown as Database["object"],
}

const target = {
  ...newBackupTarget(),
  endpoint: "http://s3.test:8333",
  bucket: "backups",
  accessKey: "id",
  secretKey: "secret",
}

describe("a new database", () => {
  test("one member on one node; MongoDB asks for three where there are more", () => {
    expect(newDatabase("postgresql", 3).members).toBe(1)
    expect(newDatabase("mongodb", 1).members).toBe(1)
    expect(newDatabase("mongodb", 3).members).toBe(3)
  })

  test("PostgreSQL is a Cluster with the chosen image, size and resources", () => {
    const [change] = createDatabase({ ...newDatabase("postgresql", 1), name: "db", version: "17" })
    expect(change).toEqual({
      op: "create",
      object: {
        apiVersion: "postgresql.cnpg.io/v1",
        kind: "Cluster",
        metadata: { name: "db", labels: { "zeabur.com/database": "db" } },
        spec: {
          instances: 1,
          imageName: "ghcr.io/cloudnative-pg/postgresql:17-standard-trixie",
          storage: { size: "5Gi" },
          resources: presetResources.postgresql.small,
        },
      },
    } as never)
  })

  test("a chosen storage class goes where each operator reads it", () => {
    const pg = createDatabase({ ...newDatabase("postgresql", 1), storageClass: "longhorn" })[0]
    expect((pg as never as { object: { spec: { storage: unknown } } }).object.spec.storage).toEqual(
      { size: "5Gi", storageClass: "longhorn" },
    )
    const mongo = createDatabase({ ...newDatabase("mongodb", 3), storageClass: "local-path" })[0]
    const replset = (
      mongo as never as { object: { spec: { replsets: [Record<string, unknown>] } } }
    ).object.spec.replsets[0]
    expect(replset.volumeSpec).toEqual({
      persistentVolumeClaim: {
        storageClassName: "local-path",
        resources: { requests: { storage: "10Gi" } },
      },
    })
  })

  test("a MongoDB replica set under three members says so, and is never sharded", () => {
    const one = createDatabase(newDatabase("mongodb", 1))[0] as never as { object: { spec: never } }
    const spec = one.object.spec as Record<string, unknown>
    expect(spec.unsafeFlags).toEqual({ replsetSize: true })
    expect(spec.sharding).toEqual({ enabled: false })
    expect(spec.users).toEqual([
      { name: "app", db: "app", roles: [{ name: "readWrite", db: "app" }] },
    ])
    const three = createDatabase(newDatabase("mongodb", 3))[0] as never as {
      object: { spec: never }
    }
    expect((three.object.spec as Record<string, unknown>).unsafeFlags).toBeUndefined()
  })

  test("a MongoDB takes its data volumes with it when deleted", () => {
    const [change] = createDatabase({ ...newDatabase("mongodb", 3), name: "mongo" }) as never as [
      { object: { metadata: unknown } },
    ]
    expect(change.object.metadata).toEqual({
      name: "mongo",
      finalizers: ["percona.com/delete-psmdb-pvc"],
      labels: { "zeabur.com/database": "mongo" },
    })
  })

  test("names are DNS labels short enough for what the operators derive", () => {
    expect(nameProblem("", [])).toBe("empty")
    expect(nameProblem("Pg", [])).toBe("invalid")
    expect(nameProblem("1pg", [])).toBe("invalid")
    expect(nameProblem("a".repeat(31), [])).toBe("long")
    expect(nameProblem("pg", ["pg"])).toBe("taken")
    expect(nameProblem("pg-main", [])).toBeUndefined()
  })

  test("a preset is recognised from the resources", () => {
    expect(presetOf("mongodb", presetResources.mongodb.large)).toBe("large")
    const { requests, limits } = presetResources.postgresql.small
    expect(presetOf("postgresql", { limits, requests })).toBe("small")
    expect(presetOf("mongodb", { requests: { cpu: "3" } })).toBeUndefined()
  })
})

const mongodEnv = (changes: unknown[]) =>
  (changes[0] as { object: { spec: { replsets: { env?: unknown }[] } } }).object.spec.replsets[0]
    ?.env

describe("MongoDB 8 memory setting", () => {
  test("a new MongoDB 8 keeps rseq in glibc; MongoDB 7 is left alone", () => {
    const eight = createDatabase({ ...newDatabase("mongodb", 1), version: "8.0" })
    expect(mongodEnv(eight)).toEqual([{ name: "GLIBC_TUNABLES", value: "glibc.pthread.rseq=1" }])
    const seven = createDatabase({ ...newDatabase("mongodb", 1), version: "7.0" })
    expect(mongodEnv(seven)).toBeUndefined()
  })

  const eight = { ...mongo, version: "8.0.32-14" }

  test("an existing MongoDB 8 without the setting gets a fix that adds it", () => {
    expect(rseqNote(eight)).toBe("missing")
    expect(rseqNote({ ...mongo, version: "7.0.43-23" })).toBeUndefined()
    const [change] = addPerThreadCaches(eight) as { yaml: string }[]
    const fixed = { ...eight, object: JSON.parse(change?.yaml ?? "{}") as Database["object"] }
    expect(rseqNote(fixed)).toBe("set")
  })

  test("the fix keeps other glibc tunables", () => {
    const tuned = structuredClone(eight)
    const spec = (tuned.object as unknown as { spec: { replsets: Record<string, unknown>[] } }).spec
    spec.replsets[0] = {
      ...spec.replsets[0],
      env: [{ name: "GLIBC_TUNABLES", value: "glibc.malloc.arena_max=2:glibc.pthread.rseq=0" }],
    }
    const [change] = addPerThreadCaches(tuned) as { yaml: string }[]
    const env = JSON.parse(change?.yaml ?? "{}").spec.replsets[0].env
    expect(env).toEqual([
      { name: "GLIBC_TUNABLES", value: "glibc.malloc.arena_max=2:glibc.pthread.rseq=1" },
    ])
  })
})

describe("backups", () => {
  test("a target needs an endpoint, a bucket, both keys and a sensible retention", () => {
    expect(targetProblems(newBackupTarget())).toEqual(["endpoint", "bucket", "keys"])
    expect(targetProblems(target)).toEqual([])
    expect(targetProblems({ ...target, keepDays: 0 })).toEqual(["keep"])
  })

  test("PostgreSQL: keys, an ObjectStore, the plugin added to the cluster, then the schedule", () => {
    const changes = enableBackups(postgres, { ...target, path: "/shop/", region: "garage" })
    expect(changes.map((c) => c.op)).toEqual(["create", "create", "edit", "create"])
    const [secret, store, edit, schedule] = changes as never as [
      { object: Record<string, never> },
      { object: { spec: Record<string, unknown> } },
      { yaml: string; resource: unknown; name: string },
      { object: { spec: Record<string, unknown> } },
    ]
    expect(secret.object).toMatchObject({
      kind: "Secret",
      metadata: { name: "pg-backups-s3" },
      stringData: { ACCESS_KEY_ID: "id", ACCESS_SECRET_KEY: "secret", REGION: "garage" },
    })
    expect(store.object.spec).toMatchObject({
      retentionPolicy: "7d",
      configuration: {
        destinationPath: "s3://backups/shop",
        endpointURL: "http://s3.test:8333",
        s3Credentials: { region: { name: "pg-backups-s3", key: "REGION" } },
      },
    })
    expect(edit.resource).toEqual({
      group: "postgresql.cnpg.io",
      version: "v1",
      resource: "clusters",
    })
    const edited = JSON.parse(edit.yaml)
    expect(edited.metadata.finalizers).toEqual(["x"])
    expect(edited.spec.plugins).toEqual([
      { name: "other" },
      {
        name: "barman-cloud.cloudnative-pg.io",
        isWALArchiver: true,
        parameters: { barmanObjectName: "pg-backups" },
      },
    ])
    expect(schedule.object.spec).toMatchObject({
      schedule: "0 0 3 * * *",
      cluster: { name: "pg" },
      method: "plugin",
    })
    expect((postgres.object as never as { spec: { plugins: unknown[] } }).spec.plugins).toEqual([
      { name: "other" },
    ])
  })

  test("MongoDB: keys, and the storage and a weekly task in the cluster's own spec", () => {
    const changes = enableBackups(mongo, { ...target, frequency: "weekly", keepDays: 30 })
    expect(changes.map((c) => c.op)).toEqual(["create", "edit"])
    const edited = JSON.parse((changes[1] as { yaml: string }).yaml)
    expect(edited.spec.backup).toEqual({
      enabled: true,
      image: "pbm:1",
      storages: {
        backups: {
          type: "s3",
          s3: {
            bucket: "backups",
            prefix: "mongo",
            endpointUrl: "http://s3.test:8333",
            region: "us-east-1",
            credentialsSecret: "mongo-backups-s3",
            forcePathStyle: true,
          },
        },
      },
      tasks: [
        {
          name: "scheduled",
          enabled: true,
          schedule: "0 3 * * 0",
          storageName: "backups",
          type: "logical",
          retention: { type: "count", count: 5, deleteFromStorage: true },
        },
      ],
    })
    expect(edited.spec.replsets).toEqual([{ name: "rs0", size: 1 }])
  })

  test("back up now names the cluster and the plugin or the storage", () => {
    const now = new Date("2026-10-22T08:09:10.123Z")
    expect(backupName("pg", now)).toBe("pg-20261022-080910")
    expect(backUpNow(postgres, now)).toEqual([
      {
        op: "create",
        object: {
          apiVersion: "postgresql.cnpg.io/v1",
          kind: "Backup",
          metadata: { name: "pg-20261022-080910", labels: { "zeabur.com/database": "pg" } },
          spec: {
            cluster: { name: "pg" },
            method: "plugin",
            pluginConfiguration: { name: "barman-cloud.cloudnative-pg.io" },
          },
        },
      } as never,
    ])
    const [mongoBackup] = backUpNow(mongo, now) as never as [{ object: { spec: unknown } }]
    expect(mongoBackup.object.spec).toEqual({
      clusterName: "mongo",
      storageName: "backups",
      type: "logical",
    })
  })
})

describe("connecting", () => {
  test("a connection string with the password left out unless given", () => {
    expect(connectionUri(postgres)).toBe("postgresql://app:PASSWORD@pg-rw.shop.svc:5432/app")
    expect(connectionUri(mongo, "p@ss")).toBe(
      "mongodb://USER:p%40ss@mongo-rs0.shop.svc:27017/admin?replicaSet=rs0",
    )
  })

  test("kubectl commands for the object and a shell in the primary", () => {
    expect(commands("shop", postgres)).toEqual({
      describe: "kubectl get clusters.postgresql.cnpg.io pg -n shop -o wide",
      shell: "kubectl exec -it -n shop pg-1 -c postgres -- psql app",
    })
    expect(commands("shop", mongo).shell).toStartWith(
      "kubectl exec -it -n shop mongo-rs0-0 -c mongod -- bash -c 'mongosh",
    )
  })
})

describe("the database's own object", () => {
  test("is its operator's custom resource", () => {
    expect(databaseAddress(postgres)).toEqual({
      group: "postgresql.cnpg.io",
      version: "v1",
      resource: "clusters",
      name: "pg",
    })
    expect(databaseAddress(mongo)).toEqual({
      group: "psmdb.percona.com",
      version: "v1",
      resource: "perconaservermongodbs",
      name: "mongo",
    })
  })
})

describe("a terminal in the database", () => {
  test("starts the engine's shell", () => {
    expect(shellCommand(postgres)).toEqual(["psql", "app"])
    expect(shellCommand(mongo).slice(0, 2)).toEqual(["bash", "-c"])
    expect(shellCommand(mongo)[2]).toStartWith("mongosh -u")
  })
})

describe("events", () => {
  const event = (kind: string, name: string) =>
    ({ regarding: { kind, name } }) as unknown as Parameters<typeof eventsOf>[0][number]
  test("are those about the object, its members and their volumes", () => {
    const withPods = { ...postgres, pods: [{ name: "pg-1" }] } as unknown as Database
    const events = [
      event("Cluster", "pg"),
      event("Cluster", "other"),
      event("Pod", "pg-1"),
      event("Pod", "pg-2"),
      event("PersistentVolumeClaim", "pg-1"),
      event("Deployment", "pg"),
    ]
    const shown = eventsOf(events, withPods).map((e) => `${e.regarding?.kind}/${e.regarding?.name}`)
    expect(shown).toEqual(["Cluster/pg", "Pod/pg-1", "PersistentVolumeClaim/pg-1"])
  })
})

describe("reshaping", () => {
  test("a volume grows and does not shrink", () => {
    expect(storageProblem("5Gi", "10Gi")).toBeUndefined()
    expect(storageProblem("5Gi", "5120Mi")).toBeUndefined()
    expect(storageProblem("5Gi", "1Gi")).toBe("shrink")
    expect(storageProblem("5Gi", "five")).toBe("invalid")
    expect(storageProblem(null, "1Gi")).toBeUndefined()
  })

  test("PostgreSQL's instances, size and storage are the Cluster's spec", () => {
    const [edit] = reshapeDatabase(postgres, { members: 3, size: "medium", storage: "10Gi" })
    expect(edit).toMatchObject({
      op: "edit",
      resource: { group: "postgresql.cnpg.io", version: "v1", resource: "clusters" },
      name: "pg",
    })
    const spec = JSON.parse((edit as { yaml: string }).yaml).spec
    expect(spec.instances).toBe(3)
    expect(spec.resources).toEqual(presetResources.postgresql.medium)
    expect(spec.storage).toEqual({ size: "10Gi" })
    expect(spec.plugins).toEqual([{ name: "other" }])
  })

  test("MongoDB's are the first replica set's, with the flags the operator asks for", () => {
    const grown = { ...mongo, storage: "2Gi" }
    const [edit] = reshapeDatabase(grown, { members: 3, size: "small", storage: "4Gi" })
    const spec = JSON.parse((edit as { yaml: string }).yaml).spec
    expect(spec.replsets[0]).toMatchObject({
      name: "rs0",
      size: 3,
      volumeSpec: { persistentVolumeClaim: { resources: { requests: { storage: "4Gi" } } } },
    })
    expect(spec.enableVolumeExpansion).toBe(true)
    expect(spec.unsafeFlags).toBeUndefined()
    const [one] = reshapeDatabase(grown, { members: 1, size: "small", storage: "2Gi" })
    const single = JSON.parse((one as { yaml: string }).yaml).spec
    expect(single.unsafeFlags).toEqual({ replsetSize: true })
    expect(single.enableVolumeExpansion).toBeUndefined()
  })

  test("deleting removes the operator's object", () => {
    expect(deleteDatabase(mongo)).toEqual([
      {
        op: "delete",
        resource: { group: "psmdb.percona.com", version: "v1", resource: "perconaservermongodbs" },
        name: "mongo",
      },
    ])
  })

  test("everything Zeabur makes for a database carries its name as a label", () => {
    const labels = (changes: ReturnType<typeof createDatabase>) =>
      changes.map((c) =>
        c.op === "create" || c.op === "copyS3Keys"
          ? (c.object as unknown as { metadata: { labels?: Record<string, string> } }).metadata
              .labels?.[DATABASE_LABEL]
          : "edit",
      )
    const mysql = createDatabase({ ...newDatabase("mysql", 1), name: "shop" }, password)
    expect(labels(mysql).every((l) => l === "shop")).toBe(true)
    const target = {
      ...newBackupTarget(),
      endpoint: "https://s3.test",
      bucket: "b",
      accessKey: "a",
      secretKey: "s",
    }
    for (const changes of [enableBackups(postgres, target), backUpNow(postgres, new Date())])
      expect(
        labels(changes)
          .filter((l) => l !== "edit")
          .every((l) => l === "pg"),
      ).toBe(true)
  })

  test("deleting also removes what Zeabur made for it, but not what a controller owns or another database reads", () => {
    const kind = (group: string, resource: string, items: { name: string; ownerKind?: string }[]) =>
      ({ group, version: "v1", resource, kind: resource, items }) as never
    const copy = {
      ...postgres,
      name: "pg-copy",
      object: { spec: { env: [{ secretKeyRef: { name: "pg-backups-s3" } }] } },
    } as unknown as Database
    const { gone, kept } = madeObjects(
      postgres,
      [
        kind("", "secrets", [{ name: "pg-backups-s3" }, { name: "pg-app-copy" }]),
        kind("postgresql.cnpg.io", "clusters", [{ name: "pg" }]),
        kind("postgresql.cnpg.io", "scheduledbackups", [{ name: "pg-daily" }]),
        kind("postgresql.cnpg.io", "backups", [{ name: "pg-1", ownerKind: "ScheduledBackup" }]),
      ],
      [postgres, copy],
    )
    expect(kept).toEqual([{ name: "pg-backups-s3", by: "pg-copy" }])
    expect(deleteDatabase(postgres, gone).map((c) => (c.op === "delete" ? c.name : c.op))).toEqual([
      "pg",
      "pg-app-copy",
      "pg-daily",
    ])
  })
})

const password = () => "pw"
const objects = (changes: ReturnType<typeof createDatabase>) =>
  changes.map((c) => (c.op === "create" ? (c.object as Record<string, any>) : {}))

describe("MySQL, MariaDB and Dragonfly", () => {
  test("MySQL is a MOCO cluster whose every container passes the restricted standard", () => {
    const form = { ...newDatabase("mysql", 3), storageClass: "fast" }
    expect(form.members).toBe(3)
    const [, cluster] = objects(createDatabase(form, password))
    expect(cluster?.kind).toBe("MySQLCluster")
    const pod = cluster?.spec.podTemplate.spec
    expect(pod.securityContext).toMatchObject({ runAsNonRoot: true, runAsUser: 10000 })
    expect(pod.containers[0]).toMatchObject({
      name: "mysqld",
      securityContext: { allowPrivilegeEscalation: false, capabilities: { drop: ["ALL"] } },
    })
    expect(cluster?.spec.overwriteContainers.map((c: { name: string }) => c.name)).toEqual([
      "agent",
      "moco-init",
      "slow-log",
    ])
    expect(cluster?.spec.volumeClaimTemplates[0].spec.storageClassName).toBe("fast")
  })

  test("MySQL gets an app database and user from a Job that reads the passwords from Secrets", () => {
    const changes = objects(createDatabase(newDatabase("mysql", 1), password))
    expect(changes.map((o) => `${o.kind}/${o.metadata.name}`)).toEqual([
      "Secret/mysql-app",
      "MySQLCluster/mysql",
      "Job/mysql-app-setup",
    ])
    expect(changes[1]?.metadata.annotations).toEqual({
      "zeabur.com/app-database": "app",
      "zeabur.com/app-secret": "mysql-app",
    })
    const setup = changes[2]?.spec.template.spec.containers[0]
    expect(setup.env.map((e: { name: string }) => e.name)).toEqual([
      "HOST",
      "WRITABLE_PASSWORD",
      "APP_PASSWORD",
    ])
    expect(setup.securityContext).toMatchObject({ allowPrivilegeEscalation: false })
  })

  test("MariaDB and Dragonfly get their passwords in Secrets Zeabur makes first", () => {
    const maria = objects(createDatabase({ ...newDatabase("mariadb", 3), members: 2 }, password))
    expect(maria.map((o) => `${o.kind}/${o.metadata.name}`)).toEqual([
      "Secret/mariadb-root",
      "Secret/mariadb-app",
      "MariaDB/mariadb",
    ])
    expect(maria[2]?.spec).toMatchObject({
      replication: { enabled: true },
      passwordSecretKeyRef: { name: "mariadb-app", key: "password", generate: false },
      podSecurityContext: { runAsNonRoot: true },
    })
    const cache = objects(createDatabase(newDatabase("dragonfly", 1), password))
    expect(cache[0]?.stringData).toEqual({ password: "pw" })
    expect(cache[1]?.spec.authentication.passwordFromSecret.name).toBe("dragonfly-auth")
    expect(cache[1]?.spec.snapshot.persistentVolumeClaimSpec.resources.requests.storage).toBe("2Gi")
  })

  test("a generated password is 24 letters and digits", () => {
    expect(newPassword()).toMatch(/^[A-Za-z0-9]{24}$/)
  })

  const as = (engine: Database["engine"], kind: string, resource: string, spec = {}) =>
    ({
      ...postgres,
      engine,
      kind,
      resource,
      name: "db",
      storage: "5Gi",
      storageClass: "fast",
      object: { kind, metadata: { name: "db" }, spec },
    }) as unknown as Database

  test("MySQL backs up through a BackupPolicy the cluster names, with its keys as AWS variables", () => {
    const changes = enableBackups(as("mysql", "MySQLCluster", "mysqlclusters"), target)
    const [keys, policy, edit] = changes
    expect(keys?.op === "create" && keys.object?.stringData).toMatchObject({
      AWS_ACCESS_KEY_ID: "id",
      AWS_REGION: "us-east-1",
    })
    expect(policy?.op === "create" && policy.object?.spec).toMatchObject({
      schedule: "0 3 * * *",
      jobConfig: {
        serviceAccountName: "default",
        bucketConfig: { bucketName: "backups", endpointURL: "http://s3.test:8333" },
        envFrom: [{ secretRef: { name: "db-backups-s3" } }],
      },
    })
    expect(edit?.op === "edit" && JSON.parse(edit.yaml).spec.backupPolicyName).toBe("db-backups")
    expect(canBackUpNow("mysql")).toBe(false)
  })

  test("MariaDB backs up through a scheduled Backup, and on demand to the same bucket", () => {
    const maria = as("mariadb", "MariaDB", "mariadbs")
    const [, backup] = enableBackups(maria, { ...target, keepDays: 3 })
    expect(backup?.op === "create" && backup.object?.spec).toMatchObject({
      schedule: { cron: "0 3 * * *" },
      maxRetention: "72h",
      storage: {
        s3: { bucket: "backups", prefix: "db", endpoint: "s3.test:8333", tls: { enabled: false } },
      },
    })
    const now = backUpNow(
      {
        ...maria,
        backups: {
          ...maria.backups,
          store: {
            name: "db-scheduled",
            destination: "s3://backups/db",
            endpoint: "https://s3.test",
            region: "eu",
            secret: "db-backups-s3",
            inUse: true,
          },
        },
      },
      new Date("2026-10-23T01:02:03Z"),
    )
    expect(now[0]?.op === "create" && now[0].object?.spec).toMatchObject({
      mariaDbRef: { name: "db" },
      storage: { s3: { bucket: "backups", prefix: "db", region: "eu", tls: { enabled: true } } },
    })
  })

  test("Dragonfly snapshots to the bucket with the endpoint as its flags", () => {
    const cache = as("dragonfly", "Dragonfly", "dragonflies", {
      args: ["--s3_endpoint=old", "--maxmemory=1gb"],
      env: [{ name: "AWS_REGION", value: "x" }],
      snapshot: { cron: "0 * * * *", persistentVolumeClaimSpec: {} },
    })
    const [, edit] = enableBackups(cache, target)
    const spec = edit?.op === "edit" ? JSON.parse(edit.yaml).spec : {}
    expect(spec.args).toEqual([
      "--maxmemory=1gb",
      "--s3_endpoint=s3.test:8333",
      "--s3_use_https=false",
    ])
    expect(spec.env.map((e: { name: string }) => e.name)).toEqual([
      "AWS_ACCESS_KEY_ID",
      "AWS_SECRET_ACCESS_KEY",
      "AWS_REGION",
    ])
    expect(spec.snapshot).toMatchObject({ dir: "s3://backups/db", persistentVolumeClaimSpec: {} })
  })

  test("reshaping MySQL changes mysqld's resources and the data claim template", () => {
    const mysql = as("mysql", "MySQLCluster", "mysqlclusters", {
      replicas: 1,
      podTemplate: { spec: { containers: [{ name: "mysqld", image: "m" }] } },
      volumeClaimTemplates: [{ metadata: { name: "mysql-data" }, spec: { resources: {} } }],
    })
    const [edit] = reshapeDatabase(mysql, { members: 3, size: "medium", storage: "8Gi" })
    const spec = edit?.op === "edit" ? JSON.parse(edit.yaml).spec : {}
    expect(spec.replicas).toBe(3)
    expect(spec.podTemplate.spec.containers[0]).toMatchObject({
      image: "m",
      resources: presetResources.mysql.medium,
    })
    expect(spec.volumeClaimTemplates[0].spec.resources.requests.storage).toBe("8Gi")
  })

  test("Dragonfly's terminal signs in with the password the operator gives the server", () => {
    const cache = as("dragonfly", "Dragonfly", "dragonflies")
    expect(commands("shop", { ...cache, primary: "db-1" }).shell).toBe(
      `kubectl exec -it -n shop db-1 -c dragonfly -- sh -c 'REDISCLI_AUTH="$DFLY_requirepass" exec redis-cli'`,
    )
  })
})

describe("the review of a backup form", () => {
  const t = ((key: string, ...args: unknown[]) =>
    args.length > 0 ? `${key}(${args.join(", ")})` : key) as unknown as Translate
  const empty = { objects: [], impact: [], warnings: [] }

  test("says the schedule, the bucket, the keys and the restart instead of the objects", () => {
    const ask = {
      changes: enableBackups(postgres, target),
      summary: backupsSummary(t, postgres, target),
    }
    expect(describePlan(t, ask, empty, () => undefined).lines).toEqual([
      "db.backup.said(pg, false, s3://backups/pg (http://s3.test:8333))",
      "db.backup.saidKeep(7)",
      "db.backup.saidKeys(pg-backups-s3)",
      "db.backup.saidRestart(pg)",
    ])
    expect(actionOf(t, ask)).toBe("db.backup.setUp")
  })

  test("a backup now names the bucket it goes to", () => {
    const stored = {
      ...postgres,
      backups: { ...postgres.backups, store: { destination: "s3://backups/pg" } },
    } as Database
    expect(backUpNowSummary(t, stored)).toEqual({
      lines: ["db.backup.saidNow(pg, s3://backups/pg)"],
      action: "db.backup.now",
    })
  })
})
