import { describe, test, expect } from "bun:test"

import type { Schemas } from "@/api/client"
import type { Translate } from "@/i18n"
import { fixLink, fixOf, fixTemplates } from "@/model/fixes"

type Gap = Schemas["Gap"]
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

describe("ready-made objects", () => {
  const policy = { kind: "NetworkPolicy", metadata: { name: "zeabur-allow-egress-api" } }
  const two = {
    ...project({}),
    services: [
      { id: "Deployment/api", kind: "Deployment", name: "api", containers: [] },
      { id: "Deployment/db", kind: "Deployment", name: "db", containers: [] },
    ],
  } as unknown as Project

  test("an egress policy is created on the sender's side", () => {
    const fix = fixTemplates["networkPolicy.allowEgress"]!
    const params = { from: "Deployment/api", to: "Deployment/db", object: JSON.stringify(policy) }
    const ask = fix(gap(params), two, t)
    expect<unknown>(ask?.changes).toStrictEqual([{ op: "create", object: policy }])
    expect(ask?.service).toStrictEqual({ kind: "Deployment", name: "api" })
    expect(fix(gap({ ...params, to: "Deployment/gone" }), two, t)).toBe(undefined)
  })

  test("a route for an Ingress is created and the Ingress is left to the person", () => {
    const fix = fixTemplates["domains.httpRoute"]!
    const route = { kind: "HTTPRoute", metadata: { name: "shop" } }
    const ask = fix(gap({ ingress: "shop", object: JSON.stringify(route) }), two, t)
    expect<unknown>(ask?.changes).toStrictEqual([{ op: "create", object: route }])
    expect(ask?.notes?.length).toBeGreaterThan(0)
    // No Gateway to attach to: the server sends no object.
    expect(fix(gap({ ingress: "shop" }), two, t)).toBe(undefined)
  })
})

describe("fixLink", () => {
  const withFix = (id: string, params: Record<string, string>, service?: string) =>
    ({ fix: { id, params }, service }) as unknown as Gap
  const shop = (mounts: { claimName: string }[], kind = "Deployment") =>
    ({
      namespace: "shop",
      services: [{ id: `${kind}/api`, kind, name: "api", containers: [{ name: "app", mounts }] }],
    }) as unknown as Project

  test("a database without backups opens its backups", () => {
    const form = fixLink(withFix("backups.scheduledBackup", { database: "main" }), shop([]))
    expect(form?.link).toMatchObject({
      params: { project: "shop", engine: "postgresql", name: "main" },
      search: { tab: "backups" },
    })
  })

  test("a volume without backups opens the storage of the service that mounts it", () => {
    const form = fixLink(
      withFix("backups.k8upSchedule", { claim: "data-api-0" }),
      shop([{ claimName: "data" }], "StatefulSet"),
    )
    expect(form?.link).toMatchObject({ params: { name: "api" }, hash: "storage" })
    const none = fixLink(withFix("backups.k8upSchedule", { claim: "other" }), shop([]))
    expect(none).toBe(undefined)
  })

  test("a declined template opens the part of Settings that sets it by hand", () => {
    const form = fixLink(withFix("probes.readiness", { service: "Deployment/api" }), shop([]))
    expect(form).toMatchObject({ label: "gap.openSettings", link: { hash: "health-checks" } })
  })
})

describe("fixOf", () => {
  test("a plan when a template writes one, else the form, and nothing without the project", () => {
    const resources = { fix: { id: "compute.resources", params: target } } as unknown as Gap
    expect(fixOf(resources, project({}), t).ask?.title).toBe("fix.resources")
    const declined = {
      fix: { id: "probes.readiness", params: target },
      service: "Deployment/api",
    } as unknown as Gap
    const shop = { ...project({}), namespace: "shop" } as Project
    expect(fixOf(declined, shop, t)).toMatchObject({ form: { label: "gap.openSettings" } })
    expect(fixOf(resources, undefined, t)).toStrictEqual({})
  })
})
