import { describe, test, expect } from "bun:test"

import type { Schemas } from "@/api/client"
import type { Translate } from "@/i18n"
import { fixLink, fixOf, fixTemplates } from "@/model/fixes"

type Gap = Schemas.Gap
type Project = Schemas.Project

const t = ((key: string) => key) as unknown as Translate

const api = { kind: "Deployment", name: "api" } as const

const gap = (fix: Partial<Schemas.Fix>, service?: string) =>
  ({ fix: { params: {}, ...fix }, service }) as unknown as Gap

const project = {
  namespace: "shop",
  services: [{ id: "Deployment/api", kind: "Deployment", name: "api", containers: [] }],
} as unknown as Project

describe("fixTemplates", () => {
  test("resources are set to the server's values on the server's workload", () => {
    const fix = fixTemplates["compute.resources"]!
    const values = {
      set: "resources" as const,
      container: "app",
      requests: { cpu: "100m", memory: "1Gi" },
      limits: { memory: "256Mi" },
    }
    const [change] = fix(gap({ workload: api, values }), project, t)?.changes ?? []
    expect(change).toStrictEqual({
      op: "resources",
      workload: api,
      container: "app",
      requests: values.requests,
      limits: values.limits,
    })
  })

  test("a readiness check is a TCP check on the port the server chose", () => {
    const fix = fixTemplates["probes.readiness"]!
    const values = {
      set: "tcpReadiness" as const,
      container: "app",
      port: 8443,
      periodSeconds: 10,
    }
    const [change] = fix(gap({ workload: api, values }), project, t)?.changes ?? []
    expect(change).toMatchObject({
      op: "probe",
      workload: api,
      container: "app",
      probe: "readiness",
      set: { handler: "TCPSocket", port: 8443, periodSeconds: 10 },
    })
  })

  test("a new disk is mounted where the server said", () => {
    const fix = fixTemplates["storage.addVolume"]!
    const values = {
      set: "volume" as const,
      container: "app",
      mountPath: "/var/lib/postgresql",
      size: "1Gi",
    }
    const ask = fix(gap({ workload: api, values }), project, t)
    expect<unknown>(ask?.changes).toStrictEqual([
      {
        op: "addVolume",
        workload: api,
        container: "app",
        volume: { mountPath: "/var/lib/postgresql", size: "1Gi" },
      },
    ])
    expect(ask?.service).toStrictEqual(api)
  })

  test("without values, or with values of another kind, a person must choose", () => {
    const volume = { set: "volume" as const, container: "app", mountPath: "/data", size: "1Gi" }
    expect(fixTemplates["storage.addVolume"]!(gap({ workload: api }), project, t)).toBe(undefined)
    expect(
      fixTemplates["compute.resources"]!(gap({ workload: api, values: volume }), project, t),
    ).toBe(undefined)
    expect(fixTemplates["storage.addVolume"]!(gap({ values: volume }), project, t)).toBe(undefined)
  })

  test("isolating a project creates every policy the server proposed, in one plan", () => {
    const fix = fixTemplates["networkPolicy.isolateProject"]!
    const objects = [
      { kind: "NetworkPolicy", metadata: { name: "zeabur-isolate-api" } },
      { kind: "NetworkPolicy", metadata: { name: "zeabur-allow-ingress-api" } },
    ]
    const ask = fix(gap({ params: { objects: JSON.stringify(objects) } }), project, t)
    expect<unknown>(ask?.changes).toStrictEqual(objects.map((object) => ({ op: "create", object })))
    expect(fix(gap({ params: { objects: "[]" } }), project, t)).toBe(undefined)
    expect(fix(gap({}), project, t)).toBe(undefined)
  })
})

describe("ready-made objects", () => {
  const policy = { kind: "NetworkPolicy", metadata: { name: "zeabur-allow-egress-api" } }
  const two = {
    ...project,
    services: [
      { id: "Deployment/api", kind: "Deployment", name: "api", containers: [] },
      { id: "Deployment/db", kind: "Deployment", name: "db", containers: [] },
    ],
  } as unknown as Project

  test("an egress policy is created on the sender's side", () => {
    const fix = fixTemplates["networkPolicy.allowEgress"]!
    const params = { from: "Deployment/api", to: "Deployment/db", object: JSON.stringify(policy) }
    const ask = fix(gap({ params }), two, t)
    expect<unknown>(ask?.changes).toStrictEqual([{ op: "create", object: policy }])
    expect(ask?.service).toStrictEqual(api)
    expect(fix(gap({ params: { ...params, to: "Deployment/gone" } }), two, t)).toBe(undefined)
  })
})

describe("fixLink", () => {
  test("a database without backups opens its backups", () => {
    const form = fixLink(
      gap({ id: "backups.scheduledBackup", params: { database: "main" } }),
      project,
    )
    expect(form?.link).toMatchObject({
      params: { project: "shop", engine: "postgresql", name: "main" },
      search: { tab: "backups" },
    })
  })

  test("a volume without backups opens the storage of the workload the server names", () => {
    const db = { kind: "StatefulSet", name: "db" } as const
    const form = fixLink(
      gap({ id: "backups.k8upSchedule", params: { claim: "data-db-0" }, workload: db }),
      project,
    )
    expect(form).toMatchObject({
      label: "gap.setUpBackups",
      link: { params: { kind: "StatefulSet", name: "db" }, hash: "storage" },
    })
    const none = fixLink(gap({ id: "backups.k8upSchedule", params: { claim: "other" } }), project)
    expect(none).toBe(undefined)
  })

  test("a declined template opens the part of Settings that sets it by hand", () => {
    const named = fixLink(gap({ id: "probes.readiness", workload: api }), project)
    expect(named).toMatchObject({ label: "gap.openSettings", link: { hash: "health-checks" } })
    const byService = fixLink(gap({ id: "domains.httpRoute" }, "Deployment/api"), project)
    expect(byService).toMatchObject({
      link: { params: { name: "api" }, search: { tab: "networking" } },
    })
  })
})

describe("fixOf", () => {
  test("a plan when a template writes one, else the form, and nothing without the project", () => {
    const values = { set: "resources" as const, container: "app", requests: {}, limits: {} }
    const resources = gap({ id: "compute.resources", workload: api, values })
    expect(fixOf(resources, project, t).ask?.title).toBe("fix.resources")
    const declined = gap({ id: "probes.readiness", workload: api })
    expect(fixOf(declined, project, t)).toMatchObject({ form: { label: "gap.openSettings" } })
    expect(fixOf(resources, undefined, t)).toStrictEqual({})
  })
})
