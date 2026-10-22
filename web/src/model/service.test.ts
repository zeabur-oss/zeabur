import { describe, test, expect } from "bun:test"

import { layout } from "@/model/layout"
import {
  canonicalKind,
  domainsOf,
  envSecretsOf,
  roleOf,
  type Network,
  type Service,
} from "@/model/service"

const service = (name: string, fields: Partial<Service> = {}) =>
  ({
    id: `projects/shop/services/deployment/${name}`,
    kind: "Deployment",
    name,
    pods: [],
    containers: [],
    endpointNames: [],
    routeNames: [],
    status: { health: "ok", word: "running" },
    ...fields,
  }) as unknown as Service

describe("a service's role", () => {
  test("is data when the server says it is a data store, whatever its image", () => {
    expect(roleOf(service("db", { image: "example/db:1", dataStore: "other" }))).toBe("data")
    // The image's name alone decides nothing here: the server classifies images.
    expect(roleOf(service("cache", { image: "redis:7" }))).toBe("worker")
  })

  test("is web before data when an Ingress exposes it", () => {
    const exposed = service("search", {
      dataStore: "search",
      endpointNames: ["search"],
      routeNames: ["public"],
    })
    expect(roleOf(exposed)).toBe("web")
  })

  test("places a data store in the canvas's data lane", () => {
    const laid = layout([service("api"), service("db", { dataStore: "postgres" })], [])
    const lanes = Object.fromEntries(laid.nodes.map((node) => [node.id, node.lane]))
    expect(lanes).toStrictEqual({
      "Deployment/api": "workers",
      "Deployment/db": "data",
    })
  })
})

describe("a service's domains", () => {
  test("come from the routes it names, and only from rules to its own Services", () => {
    const network = {
      endpoints: [],
      routes: [
        {
          name: "public",
          rules: [
            { service: "api", host: "api.example.com" },
            { service: "web", host: "example.com" },
          ],
          tlsHosts: ["api.example.com"],
        },
        { name: "other", rules: [{ service: "api", host: "other.example.com" }] },
      ],
    } as unknown as Network
    const api = service("api", { endpointNames: ["api"], routeNames: ["public"] })
    expect(domainsOf(network, api).map((d) => [d.host, d.https])).toStrictEqual([
      ["api.example.com", true],
    ])
  })
})

describe("a service kind in an address", () => {
  test("is spelled as the API spells it, whatever its case", () => {
    expect(canonicalKind("deployment")).toBe("Deployment")
    expect(canonicalKind("DEPLOYMENT")).toBe("Deployment")
    expect(canonicalKind("Deployment")).toBe("Deployment")
    expect(canonicalKind("statefulset")).toBe("StatefulSet")
  })

  test("stays as it is when no workload has that kind, so the page says not found", () => {
    expect(canonicalKind("Widget")).toBe("Widget")
  })
})

describe("the Secrets a service's variables read", () => {
  const source = (kind: string, name: string, key?: string) => ({ source: { kind, name, key } })
  test("are those named by env and envFrom once each, not the mounted ones", () => {
    const db = service("db", {
      containers: [
        {
          name: "db",
          env: [source("Secret", "users", "USER"), source("Secret", "users", "PASSWORD")],
          importedEnv: [source("Secret", "users"), source("ConfigMap", "settings")],
          mounts: [{ object: { kind: "Secret", name: "tls" } }],
        },
        {
          name: "backup",
          env: [source("Secret", "backup-s3", "KEY"), source("ConfigMap", "settings", "PORT")],
          importedEnv: [],
          mounts: [{ object: { kind: "Secret", name: "encryption-key" } }],
        },
      ],
    } as unknown as Partial<Service>)
    expect(envSecretsOf(db)).toStrictEqual(["users", "backup-s3"])
  })
})
