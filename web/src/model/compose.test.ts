import { describe, expect, test } from "bun:test"

import {
  composeChanges,
  managedOffer,
  type ComposeChoices,
  type ComposeContext,
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
  domains: { suffix: "apps.example.com" },
  enforced: "restricted",
  options: { port: true, publicUrl: true, secrets: true, volume: true },
  nodes: 1,
}

type Object = { kind: string; metadata: { name: string }; [key: string]: any }
const objects = (choices: ComposeChoices) => {
  const changes = composeChanges(conversion, choices, context)
  return {
    ops: changes.map((c) => c.op),
    objects: changes.flatMap((c) => (c.op === "create" ? [c.object as unknown as Object] : [])),
  }
}
const named = (list: Object[], kind: string, name: string) =>
  list.find((o) => o.kind === kind && o.metadata.name === name)

describe("composeChanges", () => {
  test("databases an operator makes get the connection variables of the database forms", () => {
    const { ops, objects: made } = objects({
      managed: new Set(["db", "cache"]),
      published: new Map([["web", ""]]),
    })
    expect(ops.filter((op) => op === "podSecurity")).toHaveLength(1)
    expect(named(made, "Cluster", "db")?.spec.imageName).toContain(":16-")
    expect(named(made, "Dragonfly", "cache")).toBeDefined()
    expect(named(made, "PersistentVolumeClaim", "db-db-data")).toBeUndefined()
    expect(named(made, "Ingress", "web")?.spec.rules[0].host).toBe("web-shop.apps.example.com")

    const pod = named(made, "Deployment", "web")?.spec.template.spec
    const container = pod.containers[0]
    expect(container.args).toEqual(["node", "server.js"])
    expect(container.env).toEqual([
      { name: "DATABASE_URL", valueFrom: { secretKeyRef: { name: "db-app", key: "uri" } } },
      { name: "DB_PASSWORD", valueFrom: { secretKeyRef: { name: "db-app", key: "password" } } },
      {
        name: "REDIS_PASSWORD",
        valueFrom: { secretKeyRef: { name: "cache-auth", key: "password" } },
      },
      { name: "REDIS_URL", value: "redis://:$(REDIS_PASSWORD)@cache.shop.svc:6379" },
    ])
    expect(named(made, "Secret", "web-env")?.stringData).toEqual({
      LOG: "info",
      DB_HOST: "db-rw.shop.svc:5432",
    })
    expect(pod.initContainers.map((c: any) => c.command[2])).toEqual([
      "until nc -z -w 2 cache.shop.svc 6379; do sleep 2; done",
      "until nc -z -w 2 db-rw.shop.svc 5432; do sleep 2; done",
    ])
  })

  test("a database kept as its image runs it with its usual port and volume", () => {
    const { objects: made } = objects({ managed: new Set(), published: new Map() })
    const db = named(made, "Deployment", "db")?.spec
    expect(db.replicas).toBe(1)
    expect(named(made, "Service", "db")?.spec.ports[0].port).toBe(5432)
    expect(named(made, "PersistentVolumeClaim", "db-db-data")).toBeDefined()
    expect(named(made, "Ingress", "web")).toBeUndefined()
    const env = named(made, "Secret", "web-env")?.stringData
    expect(env.DATABASE_URL).toBe("postgres://acme:s3cret@db:5432/acme")
    expect(named(made, "Deployment", "web")?.spec.replicas).toBe(3)
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
    const made = changes.flatMap((c) => (c.op === "create" ? [c.object as unknown as Object] : []))
    expect(named(made, "Seaweed", "minio")).toBeDefined()
    expect(named(made, "Bucket", "minio-uploads")?.spec.name).toBe("uploads")
    expect(named(made, "Deployment", "minio")).toBeUndefined()
    const container = named(made, "Deployment", "app")?.spec.template.spec.containers[0]
    expect(container.env).toEqual([
      {
        name: "S3_ACCESS_KEY_ID",
        valueFrom: { secretKeyRef: { name: "minio-app", key: "accessKey" } },
      },
      {
        name: "S3_SECRET_ACCESS_KEY",
        valueFrom: { secretKeyRef: { name: "minio-app", key: "secretKey" } },
      },
    ])
    expect(named(made, "Secret", "app-env")?.stringData).toEqual({
      S3_ENDPOINT: "http://minio-s3.shop.svc:8333",
      S3_BUCKET: "uploads",
    })
  })
})
