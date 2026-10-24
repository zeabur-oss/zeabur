import { describe, expect, test } from "bun:test"

import type { Schemas } from "@/api/client"
import {
  covers,
  hourOf,
  insideCluster,
  lastGood,
  locationForm,
  locationProblems,
  locationRequest,
  pickLocation,
  projectCoverage,
  scheduleForm,
  scheduleProblems,
  scheduleRequest,
  verdict,
} from "@/model/cluster-backups"

const location = (patch: Partial<Schemas["VeleroLocation"]> = {}): Schemas["VeleroLocation"] => ({
  name: "default",
  namespace: "velero",
  default: true,
  provider: "aws",
  bucket: "shop-backups",
  phase: "available",
  ...patch,
})

const schedule = (patch: Partial<Schemas["VeleroSchedule"]> = {}): Schemas["VeleroSchedule"] => ({
  name: "zeabur",
  namespace: "velero",
  cron: "0 3 * * *",
  paused: false,
  keepDays: 7,
  namespaces: [],
  excluded: [],
  fsBackup: true,
  validationErrors: [],
  zeabur: true,
  ...patch,
})

const backup = (
  outcome: Schemas["BackupOutcome"],
  name = `zeabur-${outcome}`,
): Schemas["VeleroBackup"] => ({
  name,
  namespaces: [],
  outcome,
  errors: 0,
  warnings: 0,
})

const data = (patch: Partial<Schemas["ClusterBackups"]> = {}): Schemas["ClusterBackups"] => ({
  state: "installed",
  namespace: "velero",
  locations: [location()],
  schedules: [schedule()],
  backups: [],
  restores: [],
  totalBackups: 0,
  canChange: "allowed",
  canRestore: "allowed",
  unreadable: [],
  ...patch,
})

describe("verdict", () => {
  test("Velero not installed or not readable is a state", () => {
    expect(verdict(data({ state: "not-installed" })).as).toBe("notInstalled")
    expect(verdict(data({ state: "refused" })).as).toBe("refused")
  })

  test("the location comes before the schedule", () => {
    expect(verdict(data({ locations: [] })).as).toBe("noLocation")
    expect(verdict(data({ locations: [location({ phase: "unavailable" })] })).as).toBe(
      "locationUnavailable",
    )
    expect(verdict(data({ schedules: [] })).as).toBe("noSchedule")
  })

  test("the newest backup decides, after a pause", () => {
    expect(verdict(data()).as).toBe("waiting")
    expect(verdict(data({ schedules: [schedule({ paused: true })] })).as).toBe("paused")
    const failed = data({ backups: [backup("partiallyFailed"), backup("succeeded")] })
    expect(verdict(failed)).toMatchObject({
      as: "failed",
      backup: { name: "zeabur-partiallyFailed" },
    })
    expect(verdict(data({ backups: [backup("succeeded")] })).as).toBe("ok")
    expect(verdict(data({ backups: [backup("running"), backup("failed")] })).as).toBe("running")
  })

  test("a location named default counts when none is marked default", () => {
    expect(verdict(data({ locations: [location({ default: false })] })).as).toBe("waiting")
    expect(verdict(data({ locations: [location({ default: false, name: "other" })] })).as).toBe(
      "noLocation",
    )
  })
})

describe("the location form", () => {
  test("a new location needs both keys; an edit takes both or neither", () => {
    const form = { ...locationForm(undefined), bucket: "shop-backups" }
    expect(locationProblems(form, false)).toEqual(["keys"])
    expect(locationProblems(form, true)).toEqual([])
    expect(locationProblems({ ...form, accessKeyId: "AKIA" }, true)).toEqual(["bothKeys"])
  })

  test("a destination brings its own keys and gets a folder of its own", () => {
    const destination = {
      name: "r2",
      endpoint: "https://r2.test",
      bucket: "backups",
      region: "",
      prefix: "p",
    }
    const form = { ...locationForm(undefined), ...pickLocation(destination as never) }
    expect(locationProblems(form, false)).toEqual([])
    expect(locationRequest(form)).toMatchObject({ destination: "r2", prefix: "p/velero" })
  })

  test("bucket, endpoint and folder are checked", () => {
    const form = {
      ...locationForm(location()),
      bucket: "Shop",
      endpoint: "minio:9000",
      prefix: "a/../b",
    }
    expect(locationProblems(form, true)).toEqual(["bucket", "endpoint", "prefix"])
  })

  test("the request trims slashes and spaces", () => {
    const request = locationRequest({
      bucket: " shop ",
      prefix: "/cluster/",
      region: "",
      endpoint: "https://s3.example.com/",
      accessKeyId: " id ",
      secretAccessKey: "secret",
    })
    expect(request).toMatchObject({
      op: "location",
      bucket: "shop",
      prefix: "cluster",
      endpoint: "https://s3.example.com",
      accessKeyId: "id",
    })
  })

  test("a Service or a single-label host is inside the cluster", () => {
    expect(insideCluster("http://minio.storage.svc:9000")).toBe(true)
    expect(insideCluster("http://minio.storage.svc.cluster.local:9000")).toBe(true)
    expect(insideCluster("http://minio:9000")).toBe(true)
    expect(insideCluster("https://abc.r2.cloudflarestorage.com")).toBe(false)
    expect(insideCluster("https://192.168.1.20:9000")).toBe(false)
    expect(insideCluster("")).toBe(false)
  })
})

describe("the schedule form", () => {
  test("starts daily at 03:00 UTC, kept a week, for the whole cluster", () => {
    expect(scheduleForm(undefined)).toEqual({
      hour: 3,
      keepDays: 7,
      scope: "cluster",
      namespaces: [],
    })
    expect(scheduleRequest(scheduleForm(undefined), undefined)).toEqual({
      op: "schedule",
      cron: "0 3 * * *",
      keepDays: 7,
      namespaces: [],
    })
  })

  test("a cron of another shape is kept as written", () => {
    const weekly = schedule({ cron: "30 2 * * 0" })
    const form = scheduleForm(weekly)
    expect(form.hour).toBeUndefined()
    expect(scheduleRequest({ ...form, keepDays: 14 }, weekly)).toMatchObject({
      cron: "30 2 * * 0",
      keepDays: 14,
    })
    expect(hourOf("0 23 * * *")).toBe(23)
    expect(hourOf("0 24 * * *")).toBeUndefined()
  })

  test("chosen projects need one at least, and days are 1 to 365", () => {
    const form = { ...scheduleForm(undefined), scope: "projects" as const, keepDays: 0 }
    expect(scheduleProblems(form)).toEqual(["keep", "projects"])
    expect(
      scheduleRequest({ ...form, keepDays: 7, namespaces: ["web", "shop"] }, undefined),
    ).toMatchObject({ namespaces: ["shop", "web"] })
  })
})

test("coverage and the last good backup", () => {
  expect(covers({ namespaces: [] }, "shop")).toBe(true)
  expect(covers({ namespaces: ["*"], excluded: ["shop"] }, "shop")).toBe(false)
  expect(covers({ namespaces: ["web"] }, "shop")).toBe(false)
  expect(lastGood([backup("failed"), backup("partiallyFailed"), backup("succeeded")])?.name).toBe(
    "zeabur-partiallyFailed",
  )
})

test("the project's line comes from its own read of Velero", () => {
  const project = (
    state: Schemas["ProviderState"],
    row: Partial<Schemas["BackupCoverage"]> = {},
  ): Schemas["ProjectBackups"] => ({
    providers: [{ module: "velero", state }],
    coverage: [{ target: { kind: "namespace", name: "shop" }, scheduledBy: [], ...row }],
    runs: [],
    keysFrom: {},
    schedules: [],
    unreadable: [],
    unscheduled: [],
  })
  expect(projectCoverage(project("refused"))).toBeUndefined()
  expect(projectCoverage(project("not-installed"))).toEqual({ as: "noVelero" })
  expect(projectCoverage(project("installed"))).toEqual({ as: "notCovered" })
  expect(projectCoverage(project("installed", { scheduledBy: ["velero:velero/zeabur"] }))).toEqual({
    as: "waiting",
  })
  const run = {
    name: "zeabur-1",
    provider: "velero",
    outcome: "succeeded" as const,
    finished: "2026-10-23T03:05:00Z",
    targets: [],
  }
  expect(projectCoverage(project("installed", { lastRun: run }))).toEqual({
    as: "last",
    at: "2026-10-23T03:05:00Z",
    outcome: "succeeded",
  })
})
