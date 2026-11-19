import { describe, expect, test } from "bun:test"

import type { Schemas } from "@/api/client"
import {
  composeChanges,
  differences,
  managedOffer,
  type ComposeChoices,
  type ComposeContext,
  type ComposeService,
  type Conversion,
} from "@/model/compose"

// What the server answers for a web app with PostgreSQL and Valkey (zeabur-plan's compose tests).
const conversion: Conversion = {
  waitImage: "busybox:1.37.0",
  missingEnvFiles: [],
  unset: [],
  services: [
    {
      name: "cache",
      composeName: "cache",
      image: "valkey/valkey:8",
      counterpart: "dragonfly",
      ports: [6379],
      published: false,
      variables: [],
      volumes: [],
      dependsOn: [],
      replicas: 1,
      findings: [],
    },
    {
      name: "db",
      composeName: "db",
      image: "postgres:16",
      counterpart: "postgresql",
      ports: [],
      published: false,
      variables: [{ name: "POSTGRES_PASSWORD", value: "s3cret" }],
      volumes: [{ name: "db-data", mountPath: "/var/lib/postgresql/data", readOnly: false }],
      dependsOn: [],
      replicas: 1,
      findings: [],
    },
    {
      name: "web",
      composeName: "web",
      image: "ghcr.io/acme/web:1.4",
      args: ["node", "server.js"],
      ports: [3000],
      published: true,
      variables: [
        { name: "LOG", value: "info" },
        {
          name: "DATABASE_URL",
          value: "postgres://acme:s3cret@db:5432/acme",
          refers: { service: "db", part: "url" },
        },
        { name: "DB_HOST", value: "db:5432", refers: { service: "db", part: "host" } },
        { name: "DB_PASSWORD", value: "s3cret", refers: { service: "db", part: "password" } },
        {
          name: "REDIS_URL",
          value: "redis://cache:6379",
          refers: { service: "cache", part: "url" },
        },
      ],
      volumes: [],
      dependsOn: ["cache", "db"],
      replicas: 3,
      findings: [],
    },
  ],
}

const context: ComposeContext = {
  project: "shop",
  domains: { suffixes: ["apps.example.com"] },
  options: { port: true, publicUrl: true, secrets: true, volume: true },
  nodes: 1,
}

type Object = { kind: string; metadata: { name: string }; [key: string]: any }
const objectsOf = (changes: Schemas.Change[]) =>
  changes.flatMap((c) => (c.op === "create" ? [c.object as unknown as Object] : []))
const servicesOf = (changes: Schemas.Change[]) =>
  changes.flatMap((c) => (c.op === "createImageService" ? [c] : []))
const objects = (choices: ComposeChoices) => {
  const changes = composeChanges(conversion, choices, context)
  return { objects: objectsOf(changes), services: servicesOf(changes) }
}
const named = (list: Object[], kind: string, name: string) =>
  list.find((o) => o.kind === kind && o.metadata.name === name)
const service = (list: Schemas.ImageServiceChange[], name: string) =>
  list.find((s) => s.name === name)

describe("composeChanges", () => {
  test("databases an operator makes get the connection variables of the database forms", () => {
    const { objects: made, services } = objects({
      managed: new Set(["db", "cache"]),
      published: new Map([["web", ""]]),
    })
    expect(named(made, "Cluster", "db")?.spec.imageName).toContain(":16-")
    expect(named(made, "Dragonfly", "cache")).toBeDefined()
    expect(services.map((s) => s.name)).toEqual(["web"])

    const web = service(services, "web")!
    expect(web.host).toBe("web-shop.apps.example.com")
    expect(web.args).toEqual(["node", "server.js"])
    expect(web.variables).toEqual([
      { name: "LOG", value: "info" },
      { name: "DATABASE_URL", valueFrom: { secretName: "db-app", key: "uri" } },
      { name: "DB_HOST", value: "db-rw.shop.svc:5432" },
      { name: "DB_PASSWORD", valueFrom: { secretName: "db-app", key: "password" } },
      { name: "REDIS_PASSWORD", valueFrom: { secretName: "cache-auth", key: "password" } },
      { name: "REDIS_URL", value: "redis://:$(REDIS_PASSWORD)@cache.shop.svc:6379" },
    ])
    expect(web.waitFor?.hosts).toEqual([
      { host: "cache.shop.svc", port: 6379 },
      { host: "db-rw.shop.svc", port: 5432 },
    ])
  })

  test("a database kept as its image runs it with its usual port and volume", () => {
    const { services } = objects({ managed: new Set(), published: new Map() })
    const db = service(services, "db")!
    expect(db.replicas).toBe(1)
    expect(db.port).toBe(5432)
    expect(db.volumes?.map((v) => v.name)).toEqual(["db-data"])
    const web = service(services, "web")!
    expect(web.host).toBeUndefined()
    expect(web.variables).toContainEqual({
      name: "DATABASE_URL",
      value: "postgres://acme:s3cret@db:5432/acme",
    })
    expect(web.replicas).toBe(3)
  })
})

describe("differences", () => {
  test("list what the operator's service does unlike the Compose service", () => {
    const [cache, db] = conversion.services as [ComposeService, ComposeService]
    expect(differences(conversion, db, "postgresql", { project: "shop", nodes: 1 })).toEqual([
      { kind: "volumes", paths: ["/var/lib/postgresql/data"], gib: 5 },
      { kind: "variables", names: ["POSTGRES_PASSWORD"] },
    ])
    const custom = { ...db, image: "postgres:latest", args: ["-c", "fsync=off"], ports: [15432] }
    expect(
      differences(conversion, custom, "postgresql", { project: "shop", nodes: 3 }).map(
        (d) => d.kind,
      ),
    ).toEqual(["version", "command", "ports", "volumes", "variables"])
    expect(differences(conversion, cache, "dragonfly", { project: "shop", nodes: 1 })).toEqual([
      { kind: "version", image: "valkey/valkey:8", version: expect.any(String) },
    ])
  })
})

describe("managedOffer", () => {
  const [cache, db] = conversion.services
  const yes = () => true
  test("says why a database stays an image", () => {
    expect(managedOffer(db!, { postgresql: "installed" }, yes)).toEqual({ engine: "postgresql" })
    expect(managedOffer(db!, { postgresql: "not-installed" }, yes)).toEqual({
      kept: "notInstalled",
    })
    expect(managedOffer(cache!, {}, yes)).toEqual({ kept: "unknown" })
    expect(managedOffer(db!, { postgresql: "installed" }, () => false)).toEqual({ kept: "role" })
    const minio = { ...db!, image: "minio/minio", counterpart: "objectStorage" as const }
    expect(managedOffer(minio, { objectStorage: "installed" }, yes)).toEqual({
      engine: "objectStorage",
    })
    expect(
      managedOffer(minio, { objectStorage: "installed" }, (e) => e !== "objectStorage"),
    ).toEqual({ kept: "role" })
  })
})

describe("MinIO", () => {
  const minio: Conversion = {
    waitImage: "busybox:1.37.0",
    missingEnvFiles: [],
    unset: [],
    services: [
      {
        name: "minio",
        composeName: "minio",
        image: "minio/minio:latest",
        counterpart: "objectStorage",
        ports: [9000],
        published: false,
        variables: [{ name: "MINIO_ROOT_PASSWORD", value: "s3cret" }],
        volumes: [],
        dependsOn: [],
        replicas: 1,
        findings: [],
      },
      {
        name: "app",
        composeName: "app",
        image: "ghcr.io/acme/app:1",
        ports: [],
        published: false,
        variables: [
          {
            name: "S3_ENDPOINT",
            value: "http://minio:9000",
            refers: { service: "minio", part: "url" },
          },
          { name: "S3_ACCESS_KEY_ID", value: "admin", refers: { service: "minio", part: "user" } },
          {
            name: "S3_SECRET_ACCESS_KEY",
            value: "s3cret",
            refers: { service: "minio", part: "password" },
          },
          { name: "S3_BUCKET", value: "uploads" },
        ],
        volumes: [],
        dependsOn: ["minio"],
        replicas: 1,
        findings: [],
      },
    ],
  }

  test("becomes SeaweedFS object storage with the bucket its users name", () => {
    const changes = composeChanges(
      minio,
      { managed: new Set(["minio"]), published: new Map() },
      context,
    )
    const made = objectsOf(changes)
    expect(named(made, "Seaweed", "minio")).toBeDefined()
    expect(named(made, "Bucket", "minio-uploads")?.spec.name).toBe("uploads")
    const services = servicesOf(changes)
    expect(services.map((s) => s.name)).toEqual(["app"])
    expect(service(services, "app")?.variables).toEqual([
      { name: "S3_ENDPOINT", value: "http://minio-s3.shop.svc:8333" },
      { name: "S3_ACCESS_KEY_ID", valueFrom: { secretName: "minio-app", key: "accessKey" } },
      { name: "S3_SECRET_ACCESS_KEY", valueFrom: { secretName: "minio-app", key: "secretKey" } },
      { name: "S3_BUCKET", value: "uploads" },
    ])
  })
})
