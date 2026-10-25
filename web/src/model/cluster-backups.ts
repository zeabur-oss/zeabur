// The cluster's Velero backups as the Backups tab tells them: one verdict first, then where
// backups go, when they run, and how each went.
import type { Schemas } from "@/api/client"
import { type Destination, joinFolder } from "@/model/s3-destinations"
import type { Health } from "@/model/service"

type Data = Schemas["ClusterBackups"]
type Backup = Schemas["VeleroBackup"]
type Location = Schemas["VeleroLocation"]
type Schedule = Schemas["VeleroSchedule"]

export const VELERO_COMPONENT = "volume-backup/velero"
export type Verdict =
  | { as: "notInstalled" }
  | { as: "refused" }
  | { as: "noLocation" }
  | { as: "locationUnavailable" }
  | { as: "noSchedule" }
  | { as: "paused" }
  | { as: "waiting" }
  | { as: "failed"; backup: Backup }
  | { as: "running"; backup: Backup }
  | { as: "ok"; backup: Backup }

/** The location Zeabur edits: the default one, else the one named `default`. */
export function mainLocation(data: Data): Location | undefined {
  return data.locations.find((l) => l.default) ?? data.locations.find((l) => l.name === "default")
}

/** The schedule Zeabur set up; another one is shown, not edited. */
export function mainSchedule(data: Data): Schedule | undefined {
  return data.schedules.find((s) => s.zeabur)
}

export function verdict(data: Data): Verdict {
  if (data.state === "not-installed") return { as: "notInstalled" }
  if (data.state === "refused") return { as: "refused" }
  const location = mainLocation(data)
  if (!location) return { as: "noLocation" }
  if (location.phase === "unavailable") return { as: "locationUnavailable" }
  const active = data.schedules.filter((s) => !s.paused)
  if (data.schedules.length === 0) return { as: "noSchedule" }
  const latest = data.backups.find((b) => b.outcome !== "unknown")
  if (latest?.outcome === "running") return { as: "running", backup: latest }
  if (active.length === 0) return { as: "paused" }
  if (!latest) return { as: "waiting" }
  if (latest.outcome === "failed" || latest.outcome === "partiallyFailed")
    return { as: "failed", backup: latest }
  return { as: "ok", backup: latest }
}

export const verdictHealth: Record<Verdict["as"], Health> = {
  notInstalled: "idle",
  refused: "idle",
  noLocation: "warn",
  locationUnavailable: "fail",
  noSchedule: "warn",
  paused: "idle",
  waiting: "progress",
  failed: "fail",
  running: "progress",
  ok: "ok",
}

export const outcomeHealth: Record<Schemas["BackupOutcome"], Health> = {
  succeeded: "ok",
  partiallyFailed: "warn",
  failed: "fail",
  running: "progress",
  unknown: "idle",
}

/** `0 <hour> * * *`: daily at a whole hour, the one shape the form writes. */
function dailyCron(hour: number): string {
  return `0 ${hour} * * *`
}

export function hourOf(cron: string): number | undefined {
  const match = /^0 (\d{1,2}) \* \* \*$/.exec(cron.trim())
  if (!match) return undefined
  const hour = Number(match[1])
  return hour <= 23 ? hour : undefined
}

export interface LocationForm {
  bucket: string
  prefix: string
  region: string
  endpoint: string
  accessKeyId: string
  secretAccessKey: string
  /** The S3 destination whose keys the plan copies; typed keys when absent. */
  destination?: string
}

/** The fields `destination` fills, in a folder of its own; manual entry when absent. */
export const pickLocation = (destination?: Destination): Partial<LocationForm> =>
  destination
    ? {
        destination: destination.name,
        endpoint: destination.endpoint,
        bucket: destination.bucket,
        region: destination.region,
        prefix: joinFolder(destination.prefix, "velero"),
        accessKeyId: "",
        secretAccessKey: "",
      }
    : { destination: undefined }

export function locationForm(location: Location | undefined): LocationForm {
  return {
    bucket: location?.bucket ?? "",
    prefix: location?.prefix ?? "",
    region: location?.region ?? "",
    endpoint: location?.endpoint ?? "",
    accessKeyId: "",
    secretAccessKey: "",
  }
}

export type LocationProblem = "bucket" | "endpoint" | "prefix" | "keys" | "bothKeys"

/** Keys are needed for a new location; when editing, both or neither (keep the ones it has). */
export function locationProblems(form: LocationForm, editing: boolean): LocationProblem[] {
  const problems: LocationProblem[] = []
  if (!/^[a-z0-9][a-z0-9.-]{1,61}[a-z0-9]$/.test(form.bucket.trim())) problems.push("bucket")
  const endpoint = form.endpoint.trim()
  if (endpoint && !/^https?:\/\/[^\s/]+\S*$/.test(endpoint)) problems.push("endpoint")
  if (form.prefix.includes("..") || /\s/.test(form.prefix)) problems.push("prefix")
  if (form.destination === undefined) problems.push(...keyProblems(form, editing))
  return problems
}

function keyProblems(form: LocationForm, editing: boolean): LocationProblem[] {
  const id = form.accessKeyId.trim()
  const secret = form.secretAccessKey.trim()
  if (!editing && (!id || !secret)) return ["keys"]
  return editing && Boolean(id) !== Boolean(secret) ? ["bothKeys"] : []
}

export function locationRequest(form: LocationForm): Schemas["BackupsPlanRequest"] {
  return {
    op: "location",
    bucket: form.bucket.trim(),
    prefix: form.prefix.trim().replace(/^\/+|\/+$/g, ""),
    region: form.region.trim(),
    endpoint: form.endpoint.trim().replace(/\/+$/, ""),
    accessKeyId: form.accessKeyId.trim(),
    secretAccessKey: form.secretAccessKey.trim(),
    destination: form.destination,
  }
}

/** An endpoint that names a Service or a single-label host: storage in this cluster, lost with
 * it. */
export function insideCluster(endpoint: string): boolean {
  let host: string
  try {
    host = new URL(endpoint.trim()).hostname.toLowerCase()
  } catch {
    return false
  }
  if (!host) return false
  if (host.endsWith(".svc") || host.includes(".svc.") || host.endsWith(".cluster.local"))
    return true
  return !host.includes(".") && host !== "localhost" && !host.includes(":")
}

type Scope = "cluster" | "projects"

export interface ScheduleForm {
  /** A whole hour in UTC, or `undefined` to keep a schedule written another way. */
  hour: number | undefined
  keepDays: number
  scope: Scope
  namespaces: string[]
}

const DEFAULT_HOUR = 3
const DEFAULT_KEEP_DAYS = 7

export function scheduleForm(schedule: Schedule | undefined): ScheduleForm {
  if (!schedule)
    return { hour: DEFAULT_HOUR, keepDays: DEFAULT_KEEP_DAYS, scope: "cluster", namespaces: [] }
  return {
    hour: hourOf(schedule.cron),
    keepDays: schedule.keepDays ?? DEFAULT_KEEP_DAYS,
    scope: schedule.namespaces.length === 0 ? "cluster" : "projects",
    namespaces: schedule.namespaces,
  }
}

export type ScheduleProblem = "keep" | "projects"

export function scheduleProblems(form: ScheduleForm): ScheduleProblem[] {
  const problems: ScheduleProblem[] = []
  if (!Number.isInteger(form.keepDays) || form.keepDays < 1 || form.keepDays > 365)
    problems.push("keep")
  if (form.scope === "projects" && form.namespaces.length === 0) problems.push("projects")
  return problems
}

export function scheduleRequest(
  form: ScheduleForm,
  current: Schedule | undefined,
): Schemas["BackupsPlanRequest"] {
  return {
    op: "schedule",
    cron:
      form.hour === undefined ? (current?.cron ?? dailyCron(DEFAULT_HOUR)) : dailyCron(form.hour),
    keepDays: form.keepDays,
    namespaces: form.scope === "cluster" ? [] : [...form.namespaces].sort(),
  }
}

/** When a backup was taken: when it finished, else when it started. */
export const takenAt = (backup: Backup) => backup.finished ?? backup.started

/** The newest backup that finished, with or without some items. */
export function lastGood(backups: Backup[]): Backup | undefined {
  return backups.find((b) => b.outcome === "succeeded" || b.outcome === "partiallyFailed")
}

/** Whether a namespace is in a schedule's or a backup's scope. */
export function covers(scope: { namespaces: string[]; excluded?: string[] }, namespace: string) {
  if (scope.excluded?.includes(namespace)) return false
  return (
    scope.namespaces.length === 0 ||
    scope.namespaces.includes("*") ||
    scope.namespaces.includes(namespace)
  )
}

export type ProjectCoverage =
  | { as: "noVelero" }
  | { as: "notCovered" }
  | { as: "waiting" }
  | { as: "last"; at: string | undefined; outcome: Schemas["BackupOutcome"] }

/** What Project → Settings says about the cluster's backups, from the project's own read;
 * nothing when Velero could not be read. */
export function projectCoverage(data: Schemas["ProjectBackups"]): ProjectCoverage | undefined {
  const velero = data.providers.find((p) => p.module === "velero")
  if (!velero || velero.state === "refused") return undefined
  if (velero.state === "not-installed") return { as: "noVelero" }
  const row = data.coverage.find((c) => c.target.kind === "namespace")
  const run = row?.lastRun?.provider === "velero" ? row.lastRun : undefined
  if (run) return { as: "last", at: run.finished ?? run.started ?? undefined, outcome: run.outcome }
  return row?.scheduledBy.some((s) => s.startsWith("velero:"))
    ? { as: "waiting" }
    : { as: "notCovered" }
}

/** Where a backup's contents download from: Zeabur fetches them from the location and streams
 * them, so no signed URL or key reaches the page. */
export const backupDownloadUrl = (backup: string) =>
  `/api/v1/cluster/backups/${encodeURIComponent(backup)}/download`
