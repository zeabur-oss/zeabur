import { describe, expect, test } from "bun:test"

import {
  candidates,
  connectChanges,
  connected,
  databaseOfEnv,
  disconnectChange,
  disconnection,
  nameProblem,
  namesOf,
  pendingDatabase,
  suggestedName,
  variablesFor,
  type Target,
} from "@/model/connect"
import { newDatabase, type Database } from "@/model/databases"
import { newObjectStorage, pendingObjectStorage } from "@/model/object-storage"
import type { Service } from "@/model/service"

const postgres = {
  engine: "postgresql",
  name: "pg-main",
  connection: {
    host: "pg-main-rw.shop.svc",
    port: 5432,
    secret: "pg-main-app",
    passwordKey: "password",
  },
  app: { username: "app", database: "app", secret: "pg-main-app", passwordKey: "password" },
} as Database

const mongo = {
  engine: "mongodb",
  name: "mongo",
  connection: {
    host: "mongo-rs0.shop.svc",
    port: 27017,
    secret: "mongo-secrets",
    passwordKey: "MONGODB_DATABASE_ADMIN_PASSWORD",
    options: "replicaSet=rs0",
  },
  app: { username: "app", database: "app", secret: "mongo-custom-user-secret", passwordKey: "app" },
} as Database

type Env = Service["containers"][number]["env"][number]
const literal = (name: string, value: string) =>
  ({ name, value, source: { kind: "Literal" } }) as Env
const fromSecret = (name: string, secret: string, key: string) =>
  ({ name, source: { kind: "Secret", name: secret, key } }) as Env

const web = (env: Env[] = [], containers = 1): Target => ({
  kind: "Deployment",
  name: "web",
  containers: Array.from({ length: containers }, (_, i) => ({
    name: i === 0 ? "web" : `side-${i}`,
    env: i === 0 ? env : [],
  })) as Target["containers"],
})

describe("connecting a database to a service", () => {
  test("PostgreSQL gives DATABASE_URL from the uri key, and PG* on request", () => {
    expect(variablesFor(postgres, web())).toEqual([
      { op: "set", name: "DATABASE_URL", valueFrom: { secretName: "pg-main-app", key: "uri" } },
    ])
    const names = variablesFor(postgres, web(), { extras: true }).map((e) => e.name)
    expect(names).toEqual([
      "DATABASE_URL",
      "PGHOST",
      "PGPORT",
      "PGUSER",
      "PGPASSWORD",
      "PGDATABASE",
    ])
  })

  test("MongoDB reads the password first, then builds MONGO_URL from it", () => {
    expect(variablesFor(mongo, web())).toEqual([
      {
        op: "set",
        name: "MONGOPASSWORD",
        valueFrom: { secretName: "mongo-custom-user-secret", key: "app" },
      },
      {
        op: "set",
        name: "MONGO_URL",
        value:
          "mongodb://app:$(MONGOPASSWORD)@mongo-rs0.shop.svc:27017/app?replicaSet=rs0&authSource=app",
      },
    ])
  })

  test("a name the service has gets the database's prefix", () => {
    const names = variablesFor(postgres, web([literal("DATABASE_URL", "x")]), { extras: true }).map(
      (e) => e.name,
    )
    expect(names[0]).toBe("PG_MAIN_DATABASE_URL")
    expect(names[1]).toBe("PG_MAIN_PGHOST")
    expect(variablesFor(mongo, web([literal("MONGO_URL", "x")]))[1]?.value).toContain(
      "$(MONGO_MONGOPASSWORD)",
    )
  })

  test("a chosen name; the companions follow it", () => {
    expect(
      variablesFor(postgres, web(), { name: "ORDERS_URL", extras: true }).map((e) => e.name),
    ).toEqual([
      "ORDERS_URL",
      "ORDERS_PGHOST",
      "ORDERS_PGPORT",
      "ORDERS_PGUSER",
      "ORDERS_PGPASSWORD",
      "ORDERS_PGDATABASE",
    ])
    const [password, url] = variablesFor(mongo, web(), { name: "SESSIONS_URL" })
    expect(password?.name).toBe("SESSIONS_MONGOPASSWORD")
    expect(url?.name).toBe("SESSIONS_URL")
    expect(url?.value).toContain("$(SESSIONS_MONGOPASSWORD)@")
    expect(namesOf(mongo, "MONGO_URL").password).toBe("MONGOPASSWORD")
    expect(namesOf(mongo, "DB").password).toBe("DB_MONGOPASSWORD")
  })

  test("the suggestion is free in every service, MongoDB's password too", () => {
    expect(suggestedName(postgres, [web()])).toBe("DATABASE_URL")
    const other = { ...web([literal("DATABASE_URL", "x")]), name: "api" }
    expect(suggestedName(postgres, [web(), other])).toBe("PG_MAIN_DATABASE_URL")
    expect(suggestedName(mongo, [web([literal("MONGOPASSWORD", "x")])])).toBe("MONGO_MONGO_URL")
    const both = web([literal("DATABASE_URL", "x"), literal("PG_MAIN_DATABASE_URL", "y")])
    expect(suggestedName(postgres, [both])).toBe("PG_MAIN_2_DATABASE_URL")
    expect(suggestedName(postgres, [web([literal("PGHOST", "x")])], true)).toBe(
      "PG_MAIN_DATABASE_URL",
    )
  })

  test("a name must be a variable name and free in each service", () => {
    expect(nameProblem(postgres, [web()], "DATABASE_URL")).toBeUndefined()
    expect(nameProblem(postgres, [web()], "1URL")).toEqual({ kind: "invalid" })
    expect(nameProblem(postgres, [web()], "DATABASE-URL")).toEqual({ kind: "invalid" })
    expect(nameProblem(postgres, [web([literal("DATABASE_URL", "x")])], "DATABASE_URL")).toEqual({
      kind: "taken",
      name: "DATABASE_URL",
      service: "web",
    })
    expect(nameProblem(mongo, [web([literal("MONGOPASSWORD", "x")])], "MONGO_URL")).toEqual({
      kind: "taken",
      name: "MONGOPASSWORD",
      service: "web",
    })
    expect(nameProblem(postgres, [web([literal("PGHOST", "x")])], "DATABASE_URL", true)).toEqual({
      kind: "taken",
      name: "PGHOST",
      service: "web",
    })
  })

  test("several services get the same name", () => {
    const other = { ...web([literal("DATABASE_URL", "x")]), name: "api" }
    const changes = connectChanges(postgres, [web(), other])
    expect(changes.map((c) => (c.op === "env" ? c.changes[0]?.name : ""))).toEqual([
      "PG_MAIN_DATABASE_URL",
      "PG_MAIN_DATABASE_URL",
    ])
  })

  test("disconnecting PostgreSQL removes the one variable", () => {
    const env = [
      fromSecret("DATABASE_URL", "pg-main-app", "uri"),
      fromSecret("PGHOST", "pg-main-app", "host"),
      literal("PORT", "3000"),
    ]
    expect(disconnection("DATABASE_URL", env, [postgres, mongo])).toEqual({
      database: postgres,
      names: ["DATABASE_URL"],
    })
    expect(disconnection("PORT", env, [postgres, mongo])).toBeUndefined()
    expect(disconnection("MISSING", env, [postgres])).toBeUndefined()
  })

  test("disconnecting MongoDB removes the URL and its password, from either row", () => {
    const env = [
      fromSecret("MONGOPASSWORD", "mongo-custom-user-secret", "app"),
      literal("MONGO_URL", "mongodb://app:$(MONGOPASSWORD)@mongo-rs0.shop.svc:27017/app"),
      literal("PORT", "3000"),
    ]
    const both = { database: mongo, names: ["MONGOPASSWORD", "MONGO_URL"] }
    expect(disconnection("MONGO_URL", env, [postgres, mongo])).toEqual(both)
    expect(disconnection("MONGOPASSWORD", env, [postgres, mongo])).toEqual(both)
  })

  test("a password another URL still reads stays", () => {
    const env = [
      fromSecret("MONGOPASSWORD", "mongo-custom-user-secret", "app"),
      literal("MONGO_URL", "mongodb://app:$(MONGOPASSWORD)@mongo-rs0/app"),
      literal("REPLICA_URL", "mongodb://app:$(MONGOPASSWORD)@mongo-rs0-1/app"),
    ]
    expect(disconnection("MONGO_URL", env, [mongo])?.names).toEqual(["MONGO_URL"])
  })

  test("a variable read from the database's own Secret counts, as made before naming", () => {
    const admin = fromSecret("ADMIN_PASSWORD", "mongo-secrets", "MONGODB_DATABASE_ADMIN_PASSWORD")
    expect(databaseOfEnv(admin, [admin], [postgres, mongo])?.name).toBe("mongo")
    const old = [
      fromSecret("MONGO_MONGOPASSWORD", "mongo-custom-user-secret", "app"),
      literal("MONGO_MONGO_URL", "mongodb://app:$(MONGO_MONGOPASSWORD)@mongo-rs0/app"),
    ]
    expect(disconnection("MONGO_MONGO_URL", old, [mongo])?.names).toEqual([
      "MONGO_MONGOPASSWORD",
      "MONGO_MONGO_URL",
    ])
  })

  test("the removal patch names the container only when there are several", () => {
    expect(disconnectChange(web(), "web", ["DATABASE_URL"])).toEqual({
      op: "env",
      workload: { kind: "Deployment", name: "web" },
      changes: [{ op: "remove", name: "DATABASE_URL" }],
    })
    expect(disconnectChange(web([], 2), "side-1", ["X"])).toMatchObject({ container: "side-1" })
  })

  test("a database without an app user gives nothing", () => {
    expect(variablesFor({ ...mongo, app: undefined }, web())).toEqual([])
  })

  test("several containers name the first", () => {
    const [change] = connectChanges(postgres, [web([], 2)])
    expect(change).toMatchObject({ op: "env", container: "web" })
    expect(connectChanges(postgres, [web()])[0]).not.toHaveProperty("container")
  })

  test("a variable says which database it reads, also through $(VAR)", () => {
    const password = fromSecret("MONGOPASSWORD", "mongo-custom-user-secret", "app")
    const url = literal("MONGO_URL", "mongodb://app:$(MONGOPASSWORD)@mongo-rs0.shop.svc:27017/app")
    const env = [password, url, literal("PORT", "3000")]
    expect(databaseOfEnv(url, env, [postgres, mongo])?.name).toBe("mongo")
    expect(databaseOfEnv(password, env, [postgres, mongo])?.name).toBe("mongo")
    expect(databaseOfEnv(env[2] as Env, env, [postgres, mongo])).toBeUndefined()
    expect(connected(web(env), mongo)).toBe(true)
    expect(connected(web(env), postgres)).toBe(false)
  })

  test("only services the person may patch, not the database's own or another tool's", () => {
    const service = (name: string, extra: Partial<Service> = {}) =>
      ({ kind: "Deployment", name, provenance: { warnBeforeEdit: false }, ...extra }) as Service
    const all = [
      service("web"),
      service("api"),
      service("mongo-rs0", { kind: "StatefulSet", dataStore: {} as Service["dataStore"] }),
      service("chart", {
        provenance: { warnBeforeEdit: true, tool: "Helm" } as Service["provenance"],
      }),
    ]
    const names = (rights: Parameters<typeof candidates>[1]) =>
      candidates(all, rights).map((s) => s.name)
    expect(names([{ kind: "Deployment", all: "allowed", names: [] }])).toEqual(["web", "api"])
    expect(names([{ kind: "Deployment", all: "denied", names: ["api"] }])).toEqual(["api"])
    expect(names([{ kind: "Deployment", all: "unknown", names: [] }])).toEqual(["web", "api"])
    expect(names([])).toEqual([])
  })

  test("a database created in the same plan is connected by the names its operator gives", () => {
    const pg = pendingDatabase({ ...newDatabase("postgresql", 1), name: "pg-main" }, "shop")
    expect(variablesFor(pg, web())).toEqual(variablesFor(postgres, web()))
    const mg = pendingDatabase({ ...newDatabase("mongodb", 1), name: "mongo" }, "shop")
    expect(variablesFor(mg, web())).toEqual(variablesFor(mongo, web()))
  })
})

describe("connecting MySQL, MariaDB and Dragonfly", () => {
  test("each reads its password, then builds its URL from it", () => {
    const mysql = pendingDatabase({ ...newDatabase("mysql", 1), name: "orders" }, "shop")
    expect(variablesFor(mysql, web())).toEqual([
      {
        op: "set",
        name: "MYSQLPASSWORD",
        valueFrom: { secretName: "orders-app", key: "password" },
      },
      {
        op: "set",
        name: "DATABASE_URL",
        value: "mysql://app:$(MYSQLPASSWORD)@moco-orders-primary.shop.svc:3306/app",
      },
    ])
    const maria = pendingDatabase({ ...newDatabase("mariadb", 1), members: 2 }, "shop")
    expect(variablesFor(maria, web()).at(-1)?.value).toBe(
      "mysql://app:$(MYSQLPASSWORD)@mariadb-primary.shop.svc:3306/app",
    )
    const cache = pendingDatabase(newDatabase("dragonfly", 1), "shop")
    expect(variablesFor(cache, web(), { name: "CACHE_REDIS_URL" })).toEqual([
      {
        op: "set",
        name: "CACHE_REDIS_PASSWORD",
        valueFrom: { secretName: "dragonfly-auth", key: "password" },
      },
      {
        op: "set",
        name: "CACHE_REDIS_URL",
        value: "redis://:$(CACHE_REDIS_PASSWORD)@dragonfly.shop.svc:6379",
      },
    ])
  })
})

describe("connecting object storage", () => {
  const store = pendingObjectStorage({ ...newObjectStorage(), buckets: ["uploads"] }, "shop")
  const app = web()

  test("gives the endpoint, both keys from the Secret, the region and the bucket", () => {
    expect(variablesFor(store, app, { bucket: "uploads" })).toEqual([
      { op: "set", name: "S3_ENDPOINT", value: "http://files-s3.shop.svc:8333" },
      {
        op: "set",
        name: "S3_ACCESS_KEY_ID",
        valueFrom: { secretName: "files-app", key: "accessKey" },
      },
      {
        op: "set",
        name: "S3_SECRET_ACCESS_KEY",
        valueFrom: { secretName: "files-app", key: "secretKey" },
      },
      { op: "set", name: "S3_REGION", value: "us-east-1" },
      { op: "set", name: "S3_BUCKET", value: "uploads" },
    ])
  })

  test("a taken endpoint name gets the store's prefix, the keys too", () => {
    const taken = web([literal("S3_BUCKET", "x")])
    const name = suggestedName(store, [taken])
    expect(name).toBe("FILES_S3_ENDPOINT")
    expect(namesOf(store, name).extras).toEqual([
      "FILES_S3_ACCESS_KEY_ID",
      "FILES_S3_REGION",
      "FILES_S3_BUCKET",
    ])
  })
})
