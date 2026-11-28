import { describe, expect, test } from "bun:test"

import type { Schemas } from "@/api/client"
import {
  covers,
  insideCluster,
  lastGood,
  locationForm,
  locationProblems,
  locationRequest,
  pickLocation,
  scheduleForm,
  scheduleProblems,
  scheduleRequest,
} from "@/model/cluster-backups"

const location = (patch: Partial<Schemas.VeleroLocation> = {}): Schemas.VeleroLocation => ({
  name: "default",
  namespace: "velero",
  default: true,
  provider: "aws",
  bucket: "shop-backups",
  phase: "available",
  ...patch,
})

const schedule = (patch: Partial<Schemas.VeleroSchedule> = {}): Schemas.VeleroSchedule => ({
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
  outcome: Schemas.BackupOutcome,
  name = `zeabur-${outcome}`,
): Schemas.VeleroBackup => ({
  name,
  namespaces: [],
  outcome,
  errors: 0,
  warnings: 0,
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
      time: { weekly: false, day: 0, minute: 180 },
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
    const monthly = schedule({ cron: "30 2 1 * *" })
    const form = scheduleForm(monthly)
    expect(form.time).toBeUndefined()
    expect(scheduleRequest({ ...form, keepDays: 14 }, monthly)).toMatchObject({
      cron: "30 2 1 * *",
      keepDays: 14,
    })
  })

  test("any minute of a day or a week is a time the form writes back", () => {
    const weekly = schedule({ cron: "45 17 * * 6" })
    const form = scheduleForm(weekly)
    expect(form.time).toEqual({ weekly: true, day: 6, minute: 17 * 60 + 45 })
    expect(scheduleRequest(form, weekly)).toMatchObject({ cron: "45 17 * * 6" })
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
