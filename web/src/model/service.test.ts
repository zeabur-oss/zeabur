import { describe, test, expect } from "bun:test"

import { layout } from "@/model/layout"
import { canonicalKind, roleOf, type Service } from "@/model/service"

const service = (name: string, fields: Partial<Service> = {}) =>
  ({
    id: `projects/shop/services/deployment/${name}`,
    kind: "Deployment",
    name,
    pods: [],
    containers: [],
    endpoints: [],
    routes: [],
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
      endpoints: [{ name: "search" }],
      routes: [{ name: "public", rules: [{ service: "search", host: "example.com" }] }],
    } as unknown as Partial<Service>)
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
