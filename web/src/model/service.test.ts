import { describe, test, expect } from "bun:test"

import { layout } from "@/model/layout"
import {
  canonicalKind,
  domainsOf,
  envSecretsOf,
  groupServices,
  imageParts,
  imageTag,
  serviceOfPod,
  type Network,
  type Pod,
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
    role: "worker",
    ...fields,
  }) as unknown as Service

describe("a service's role", () => {
  test("places a data store in the canvas's data lane", () => {
    const db = service("db", { dataStore: "postgres", role: "data" })
    const laid = layout([service("api"), db], [])
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
          origin: { kind: "Ingress", name: "public" },
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

describe("the service of a pod", () => {
  test("is the one that lists the pod, so a delete names the controller that replaces it", () => {
    const pods = (...names: string[]) => names.map((name) => ({ name }) as Pod)
    const services = [
      service("web", { pods: pods("web-7d9-abc") }),
      service("db", { kind: "StatefulSet", pods: pods("db-0") }),
    ]
    expect(serviceOfPod(services, "db-0")?.name).toBe("db")
    expect(serviceOfPod(services, "web-7d9-abc")?.kind).toBe("Deployment")
    expect(serviceOfPod(services, "other-0")).toBe(undefined)
  })

  test("is none for a Job or CronJob pod, which no controller makes again after a delete", () => {
    const pods = (...names: string[]) => names.map((name) => ({ name }) as Pod)
    const services = [
      service("migrate", { kind: "Job", pods: pods("migrate-x7k2p") }),
      service("nightly", { kind: "CronJob", pods: pods("nightly-29012-abcde") }),
      service("agent", { kind: "DaemonSet", pods: pods("agent-q8w9e") }),
    ]
    expect(serviceOfPod(services, "migrate-x7k2p")).toBe(undefined)
    expect(serviceOfPod(services, "nightly-29012-abcde")).toBe(undefined)
    expect(serviceOfPod(services, "agent-q8w9e")?.name).toBe("agent")
  })
})

describe("an image reference", () => {
  const DIGEST = `sha256:${"0123456789abcdef".repeat(4)}`

  test("drops the registry host and keeps the path and tag", () => {
    expect(imageParts("registry.example.com:5000/team-a/web:main-1795667530-a2e57e2")).toEqual({
      repository: "team-a/web",
      tag: "main-1795667530-a2e57e2",
      digest: undefined,
    })
  })

  test("keeps a Docker Hub path, which has no host", () => {
    expect(imageParts("library/nginx:1.27")).toMatchObject({
      repository: "library/nginx",
      tag: "1.27",
    })
    expect(imageParts("nginx")).toMatchObject({ repository: "nginx", tag: undefined })
  })

  test("reads a tag and a digest together", () => {
    expect(imageParts(`localhost/team-a/web:main-1@${DIGEST}`)).toEqual({
      repository: "team-a/web",
      tag: "main-1",
      digest: "0123456789abcdef".repeat(4),
    })
  })

  test("prefers the tag, then a short digest, then latest", () => {
    expect(imageTag(`r.example.com/web:main-1@${DIGEST}`)).toBe("main-1")
    expect(imageTag(`r.example.com/web@${DIGEST}`)).toBe("0123456789ab")
    expect(imageTag("r.example.com:5000/web")).toBe("latest")
  })
})

describe("grouping services in the sidebar", () => {
  const names = (services: Service[]) =>
    groupServices(services).map((g) => [g.name ?? g.role ?? "", g.services.map((s) => s.name)])

  test("one named group lists the rest as ungrouped", () => {
    const shop = service("shop", { group: "shop", grouped: true })
    const api = service("api", { group: "default" })
    expect(names([api, shop])).toStrictEqual([
      ["shop", ["shop"]],
      ["", ["api"]],
    ])
  })

  test("a chart's component label groups by role", () => {
    const db = service("db", { group: "primary", role: "data" })
    const api = service("api", { group: "server" })
    expect(names([api, db])).toStrictEqual([
      ["worker", ["api"]],
      ["data", ["db"]],
    ])
  })
})
