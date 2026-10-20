import { describe, test, expect } from "bun:test"

import type { Schemas } from "@/api/client"
import type { Translate } from "@/i18n"
import { fixTemplates } from "@/model/fixes"

type Gap = Schemas["GapView"]
type Project = Schemas["Project"]

const t = ((key: string) => key) as unknown as Translate

const gap = (params: Record<string, string>) => ({ fix: { params } }) as unknown as Gap

/** A project of one Deployment `api` with one container `app`. */
const project = (
  container: Record<string, unknown>,
  ports: { port: number; targetPort?: string }[] = [],
) =>
  ({
    services: [
      {
        id: "Deployment/api",
        kind: "Deployment",
        name: "api",
        containers: [{ name: "app", ...container }],
        endpointNames: ["api"],
        routeNames: [],
      },
    ],
    endpoints: [{ name: "api", ports }],
    routes: [],
  }) as unknown as Project

const target = { service: "Deployment/api", container: "app" }

describe("fixTemplates", () => {
  test("resources fill in only what is missing; what the container sets is kept", () => {
    const fix = fixTemplates["compute.resources"]!
    const [change] = fix(gap(target), project({}), t)?.changes ?? []
    expect(change).toStrictEqual({
      op: "resources",
      workload: { kind: "Deployment", name: "api" },
      container: "app",
      requests: { cpu: "100m", memory: "128Mi" },
      limits: { memory: "256Mi" },
    })
    const own = project({ resources: { requests: { memory: "1Gi" }, limits: { cpu: "2" } } })
    const [kept] = fix(gap(target), own, t)?.changes ?? []
    expect(kept && "requests" in kept && [kept.requests, kept.limits]).toStrictEqual([
      { cpu: "100m", memory: "1Gi" },
      { cpu: "2", memory: "256Mi" },
    ])
    // A service or container that is not there: nothing to propose.
    expect(fix(gap({ ...target, container: "gone" }), own, t)).toBe(undefined)
  })

  test("a readiness check takes the first numeric port, and none without one", () => {
    const fix = fixTemplates["probes.readiness"]!
    const named = project({}, [{ port: 80, targetPort: "http" }])
    expect(fix(gap(target), named, t)).toBe(undefined)
    const numeric = project({}, [
      { port: 80, targetPort: "http" },
      { port: 443, targetPort: "8443" },
    ])
    const [change] = fix(gap(target), numeric, t)?.changes ?? []
    expect(change && "set" in change && change.set).toStrictEqual({
      handler: "TCPSocket",
      port: 8443,
      periodSeconds: 10,
    })
    // Without a targetPort, the Service sends to its own port.
    const [same] = fix(gap(target), project({}, [{ port: 8080 }]), t)?.changes ?? []
    expect(same && "set" in same && same.set?.port).toBe(8080)
  })

  test("isolating a project creates every policy the server proposed, in one plan", () => {
    const fix = fixTemplates["networkPolicy.isolateProject"]!
    const objects = [
      { kind: "NetworkPolicy", metadata: { name: "zeabur-isolate-api" } },
      { kind: "NetworkPolicy", metadata: { name: "zeabur-allow-ingress-api" } },
    ]
    const shop = { ...project({}), namespace: "shop" } as Project
    const ask = fix(gap({ objects: JSON.stringify(objects) }), shop, t)
    expect<unknown>(ask?.changes).toStrictEqual(objects.map((object) => ({ op: "create", object })))
    expect(fix(gap({ objects: "[]" }), shop, t)).toBe(undefined)
    expect(fix(gap({}), shop, t)).toBe(undefined)
  })
})

describe("storage.addVolume", () => {
  const fix = fixTemplates["storage.addVolume"]!
  const params = { ...target, mountPath: "/var/lib/postgresql" }

  test("puts the data directory on a new disk", () => {
    const [change] = fix(gap(params), project({ mounts: [] }), t)?.changes ?? []
    expect(change).toStrictEqual({
      op: "addVolume",
      workload: { kind: "Deployment", name: "api" },
      container: "app",
      volume: { mountPath: "/var/lib/postgresql", size: "1Gi" },
    })
  })

  test("leaves a temporary volume at that path to a person", () => {
    const mounted = project({ mounts: [{ mountPath: "/var/lib/postgresql", kind: "EmptyDir" }] })
    expect(fix(gap(params), mounted, t)).toBeUndefined()
  })
})
