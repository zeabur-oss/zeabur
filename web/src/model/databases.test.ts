import { describe, expect, test } from "bun:test"

import {
  allowedVersion,
  backUpNow,
  backupName,
  blockedVersions,
  commands,
  connectionUri,
  createDatabase,
  enableBackups,
  nameProblem,
  newBackupTarget,
  newDatabase,
  presetOf,
  presetResources,
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

  test("MongoDB 8 is not offered on Linux 6.19 and newer", () => {
    expect(blockedVersions("mongodb", ["6.8.0-45-generic"])).toEqual([])
    expect(blockedVersions("mongodb", ["6.8.0-45-generic", "6.19.2"])).toEqual(["8.0"])
    expect(blockedVersions("mongodb", ["7.0.0-38-generic"])).toEqual(["8.0"])
    expect(blockedVersions("postgresql", ["7.0.0-38-generic"])).toEqual([])
    expect(newDatabase("mongodb", 1, ["8.0"]).version).toBe("7.0")
  })

  test("a version the kernels refuse moves to one they allow, as nodes load after the form", () => {
    expect(allowedVersion("mongodb", "8.0", ["8.0"])).toBe("7.0")
    expect(allowedVersion("mongodb", "8.0", [])).toBe("8.0")
    expect(allowedVersion("mongodb", "7.0", ["8.0"])).toBe("7.0")
  })

  test("PostgreSQL is a Cluster with the chosen image, size and resources", () => {
    const [change] = createDatabase({ ...newDatabase("postgresql", 1), name: "db", version: "17" })
    expect(change).toEqual({
      op: "create",
      object: {
        apiVersion: "postgresql.cnpg.io/v1",
        kind: "Cluster",
        metadata: { name: "db" },
        spec: {
          instances: 1,
          imageName: "ghcr.io/cloudnative-pg/postgresql:17-standard-trixie",
          storage: { size: "5Gi" },
          resources: presetResources.postgresql.small,
        },
      },
    } as never)
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
          metadata: { name: "pg-20261022-080910" },
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
