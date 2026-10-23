import { describe, expect, test } from "bun:test"

import {
  databaseEdges,
  databaseNodes,
  engineVersion,
  foldDatabases,
  storageRows,
  versionLabel,
} from "@/model/database-parts"
import type { Database } from "@/model/databases"
import type { Claim, Project, Service } from "@/model/service"

const mongo = {
  engine: "mongodb",
  name: "mongo",
  state: "ready",
  version: "7.0.43-23",
  claims: ["mongod-data-mongo-rs0-0"],
  app: { username: "app", database: "app", secret: "mongo-custom-user-secret", passwordKey: "app" },
} as Database

const pg = {
  engine: "postgresql",
  name: "pg",
  state: "ready",
  version: "17.2",
  storage: "5Gi",
  claims: ["pg-1"],
  app: { username: "app", database: "app", secret: "pg-app", passwordKey: "password" },
} as Database

const service = (name: string, fields: Partial<Service> = {}) =>
  ({
    kind: "Deployment",
    name,
    containers: [],
    pods: [],
    ...fields,
  }) as unknown as Service

const rs0 = service("mongo-rs0", {
  kind: "StatefulSet",
  managedBy: { kind: "PerconaServerMongoDB", name: "mongo", engine: "MongoDB" },
} as Partial<Service>)

const reader = (name: string, secret: string) =>
  service(name, {
    containers: [
      { name, env: [{ name: "URL", source: { kind: "Secret", name: secret, key: "uri" } }] },
    ],
  } as unknown as Partial<Service>)

const api = reader("api", "pg-app")
const editor = reader("editor", "mongo-custom-user-secret")

const project = {
  services: [api, editor, rs0],
  connections: [
    {
      from: { kind: "Deployment", name: "editor" },
      to: { kind: "StatefulSet", name: "mongo-rs0" },
      inferred: true,
      evidence: [],
    },
    {
      from: { kind: "Deployment", name: "api" },
      to: { kind: "Deployment", name: "editor" },
      inferred: false,
      evidence: [],
    },
  ],
  resources: [],
} as unknown as Project

const claim = (name: string, services: Service[], fields: Partial<Claim> = {}): Claim => ({
  name,
  services,
  orphan: false,
  ...fields,
})

describe("a database's own workloads", () => {
  test("are not services of the project, nor ends of its connections", () => {
    const folded = foldDatabases(project)
    expect(folded.services.map((s) => s.name)).toStrictEqual(["api", "editor"])
    expect(folded.connections.map((c) => c.to.name)).toStrictEqual(["editor"])
  })

  test("leave a project without any as it is", () => {
    const plain = { ...project, services: [api], connections: [] }
    expect(foldDatabases(plain)).toBe(plain)
  })
})

describe("the map's databases", () => {
  test("take the connections to their workloads and each reader of the app Secret", () => {
    const edges = databaseEdges(project, [mongo, pg])
    expect(edges.map((e) => `${e.from} ${e.to}`).toSorted()).toStrictEqual([
      "Deployment/api database:postgresql/pg",
      "Deployment/editor database:mongodb/mongo",
    ])
    expect(edges.find((e) => e.to === "database:mongodb/mongo")?.inferred).toBe(true)
  })

  test("show each member's volume with the claim's size when the database names none", () => {
    const [node] = databaseNodes(
      [mongo],
      [claim("mongod-data-mongo-rs0-0", [rs0], { size: "10Gi" })],
    )
    expect(node?.volumes).toStrictEqual([{ name: "mongod-data-mongo-rs0-0", size: "10Gi" }])
    expect(node?.health).toBe("ok")
  })
})

describe("the Storage card", () => {
  test("lists a database's volumes as one row under the database", () => {
    const rows = storageRows(
      [
        claim("mongod-data-mongo-rs0-0", [rs0], { sizeBytes: 10 * 2 ** 30 }),
        claim("pg-1", []),
        claim("uploads", [api], { sizeBytes: 2 ** 30 }),
      ],
      [mongo, pg],
    )
    expect(rows.map((r) => [r.key, r.database?.name, r.bytes])).toStrictEqual([
      ["database:mongodb/mongo", "mongo", 10 * 2 ** 30],
      ["database:postgresql/pg", "pg", 5 * 2 ** 30],
      ["uploads", undefined, 2 ** 30],
    ])
    expect(rows[0]?.claims[0]?.services).toStrictEqual([])
  })

  test("keeps a database's volume that a service also mounts as its own row", () => {
    const rows = storageRows([claim("pg-1", [api])], [pg])
    expect(rows.map((r) => [r.key, r.database])).toStrictEqual([["pg-1", undefined]])
  })
})

describe("a database's version", () => {
  test("reads as the create form offers it", () => {
    expect(versionLabel("mongodb", "7.0.43-23")).toBe("7.0")
    expect(versionLabel("postgresql", "17.2")).toBe("17")
    expect(versionLabel("postgresql", "latest")).toBe("latest")
    expect(versionLabel("mongodb", undefined)).toBe("")
    expect(engineVersion(mongo)).toBe("MongoDB 7.0")
    expect(engineVersion({ engine: "postgresql", version: null })).toBe("PostgreSQL")
  })
})
