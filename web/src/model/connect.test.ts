import { describe, expect, test } from "bun:test"

import {
  candidates,
  connectChanges,
  connected,
  databaseOfEnv,
  pendingDatabase,
  variablesFor,
  type Target,
} from "@/model/connect"
import { newDatabase, type Database } from "@/model/databases"
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
