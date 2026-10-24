// A volume's backups with K8up: a Secret with the bucket's keys and the restic repository
// password, the claim labelled for selection, and a `Schedule` that backs up that claim alone and
// prunes by its retention.
// https://docs.k8up.io/k8up/2.16/references/api-reference.html
import type { Schemas } from "@/api/client"
import {
  backupName,
  newPassword,
  targetProblems,
  type BackupTarget,
  type TargetProblem,
} from "@/model/databases"
import { AWS_KEYS, keysSecret } from "@/model/s3-destinations"

type Change = Schemas["Change"]
type ProjectBackups = Schemas["ProjectBackups"]
type Schedule = Schemas["BackupSchedule"]
type Service = Pick<Schemas["Service"], "runAs" | "podSecurity">

/** Who K8up's backup and prune pods run as: the service's own user. */
export type BackupUser =
  | { root: true }
  | { root: false; user?: number; group?: number; fsGroup?: number }

/** Root only when the service may run as root itself: it sets user 0, or it sets no user and
 * does not meet the restricted level. */
export function backupUser(service: Service): BackupUser {
  const { user, group, fsGroup } = service.runAs ?? {}
  if (user === 0 || (user == null && service.podSecurity !== "restricted")) return { root: true }
  return {
    root: false,
    ...(user == null ? {} : { user }),
    ...(group == null ? {} : { group }),
    ...(fsGroup == null ? {} : { fsGroup }),
  }
}

/** A PodConfig that K8up merges into its pods; the container's name is K8up's to set. */
function restrictedPods(name: string, as: Extract<BackupUser, { root: false }>) {
  return {
    apiVersion: "k8up.io/v1",
    kind: "PodConfig",
    metadata: { name },
    spec: {
      template: {
        spec: {
          securityContext: {
            runAsNonRoot: true,
            ...(as.user === undefined ? {} : { runAsUser: as.user }),
            ...(as.group === undefined ? {} : { runAsGroup: as.group }),
            ...(as.fsGroup === undefined
              ? {}
              : { fsGroup: as.fsGroup, fsGroupChangePolicy: "OnRootMismatch" }),
            seccompProfile: { type: "RuntimeDefault" },
          },
          containers: [
            {
              name: "backup",
              securityContext: {
                allowPrivilegeEscalation: false,
                capabilities: { drop: ["ALL"] },
              },
            },
          ],
        },
      },
    },
  }
}

/** K8up backs up only annotated claims when installed from the catalogue. */
const BACKUP_ANNOTATION = "k8up.io/backup"
/** The label a volume's `Schedule` selects its claim by. */
const VOLUME_LABEL = "zeabur.com/backup-volume"

export interface VolumeTarget extends BackupTarget {
  /** The newest backups to keep, whatever their age. */
  keepLast: number
  /** One backup a week, for this many weeks. */
  keepWeekly: number
}

export const newVolumeTarget = (): VolumeTarget => ({
  endpoint: "",
  bucket: "",
  path: "",
  region: "",
  accessKey: "",
  secretKey: "",
  frequency: "daily",
  keepDays: 7,
  keepLast: 3,
  keepWeekly: 4,
})

const count = (n: number, max: number) => Number.isInteger(n) && n >= 0 && n <= max

export function volumeTargetProblems(target: VolumeTarget): TargetProblem[] {
  const problems = targetProblems(target)
  if (!problems.includes("keep") && !(count(target.keepLast, 100) && count(target.keepWeekly, 520)))
    problems.push("keep")
  return problems
}

/** A label value has at most 63 characters. */
export const labelable = (claim: string) => claim.length <= 63

const names = (claim: string) => ({ secret: `${claim}-backup`, schedule: `${claim}-backup` })

/** The bucket and folder, as K8up's `bucket` field takes them: restic reads a path after it. */
const repository = (target: VolumeTarget, project: string, claim: string) =>
  `${target.bucket.trim()}/${target.path.trim().replace(/^\/+|\/+$/g, "") || `${project}/${claim}`}`

const create = (object: Record<string, unknown>): Change => ({
  op: "create",
  object: object as Record<string, never>,
})

/** The writes that back `claim` up on a schedule, to `target`, as `as`. */
export function scheduleVolumeBackups(
  project: string,
  claim: string,
  target: VolumeTarget,
  as: BackupUser,
  password = newPassword,
): Change[] {
  const { secret, schedule } = names(claim)
  const region = target.region.trim()
  const cron = target.frequency === "weekly" ? "0 3 * * 0" : "0 3 * * *"
  return [
    keysSecret(secret, target, AWS_KEYS, {
      RESTIC_PASSWORD: password(),
      ...(region ? { AWS_DEFAULT_REGION: region } : {}),
    }),
    ...(as.root ? [] : [create(restrictedPods(schedule, as))]),
    {
      op: "labelVolume",
      claim,
      labels: { [VOLUME_LABEL]: claim },
      annotations: { [BACKUP_ANNOTATION]: "true" },
    },
    create({
      apiVersion: "k8up.io/v1",
      kind: "Schedule",
      metadata: { name: schedule },
      spec: {
        backend: {
          repoPasswordSecretRef: { name: secret, key: "RESTIC_PASSWORD" },
          s3: {
            endpoint: target.endpoint.trim().replace(/\/+$/, ""),
            bucket: repository(target, project, claim),
            accessKeyIDSecretRef: { name: secret, key: "AWS_ACCESS_KEY_ID" },
            secretAccessKeySecretRef: { name: secret, key: "AWS_SECRET_ACCESS_KEY" },
          },
          ...(region ? { envFrom: [{ secretRef: { name: secret } }] } : {}),
        },
        // Backup and prune pods run as the service does.
        ...(as.root
          ? { podSecurityContext: { runAsUser: 0 } }
          : { podConfigRef: { name: schedule } }),
        backup: {
          schedule: cron,
          labelSelectors: [{ matchLabels: { [VOLUME_LABEL]: claim } }],
        },
        prune: {
          schedule: target.frequency === "weekly" ? "0 4 * * 0" : "0 4 * * *",
          retention: {
            keepLast: target.keepLast,
            keepDaily: target.keepDays,
            keepWeekly: target.keepWeekly,
          },
        },
      },
    }),
  ]
}

const coversClaim = (schedule: Schedule, claim: string) =>
  schedule.targets.some((t) => t.kind === "volume" && t.name === claim)

/** The K8up schedule that backs `claim` up, preferring the one made for it alone. */
export function scheduleOf(data: ProjectBackups, claim: string): Schedule | undefined {
  const k8up = data.schedules.filter((s) => s.provider === "k8up" && coversClaim(s, claim))
  return k8up.find((s) => s.name.endsWith(`/${names(claim).schedule}`)) ?? k8up[0]
}

/** Whether `schedule` is the one made for `claim` alone, so stopping it stops nothing else. */
export const ownSchedule = (schedule: Schedule, claim: string) =>
  schedule.name.endsWith(`/${names(claim).schedule}`)

/** The deletes that stop `schedule`. The Secret stays: its password restores the backups taken. */
export function stopVolumeBackups(schedule: Schedule): Change[] {
  const name = schedule.name.slice(schedule.name.indexOf("/") + 1)
  const runNow = schedule.runNow as { spec?: { podConfigRef?: { name?: string } } } | undefined
  const podConfig = runNow?.spec?.podConfigRef?.name
  const k8up = (resource: string) => ({ group: "k8up.io", version: "v1", resource })
  return [
    { op: "delete", resource: k8up("schedules"), name },
    ...(podConfig
      ? [{ op: "delete" as const, resource: k8up("podconfigs"), name: podConfig }]
      : []),
  ]
}

/** One backup of `claim` now, as its schedule would take it. */
export function backUpVolumeNow(schedule: Schedule, claim: string, now: Date): Change[] {
  const object = schedule.runNow as Record<string, unknown> | undefined
  if (!object) return []
  return [create({ ...object, metadata: { name: backupName(claim, now) } })]
}

/** The newest runs that covered `claim`. */
export const runsOf = (data: ProjectBackups, claim: string) =>
  data.runs.filter((r) => r.targets.some((t) => t.kind === "volume" && t.name === claim))

export const k8upState = (data: ProjectBackups) =>
  data.providers.find((p) => p.module === "k8up")?.state ?? "not-installed"
