// Databases an operator runs: the objects Zeabur's forms create, and what the pages show of them.
//
// PostgreSQL is a CloudNativePG `Cluster`; its backups go through the Barman Cloud plugin (an
// `ObjectStore` with the S3 target, named in the cluster's `spec.plugins`, and `ScheduledBackup` /
// `Backup` objects with `method: plugin`). The in-tree `barmanObjectStore` is deprecated and gone in
// CloudNativePG 1.32.
// https://cloudnative-pg.io/plugin-barman-cloud/docs/usage/
//
// MongoDB is a `PerconaServerMongoDB` replica set without sharding; its backups are Percona Backup
// for MongoDB, configured in `spec.backup` (a storage and scheduled tasks), and an on-demand backup
// is a `PerconaServerMongoDBBackup`.
// https://docs.percona.com/percona-operator-for-mongodb/backups.html
import type { Schemas } from "@/api/client"
import type { ObjectAddress } from "@/api/queries"
import type { Event, Health } from "@/model/service"

export type Database = Schemas["Database"]
export type Engine = Schemas["DatabaseEngine"]
type Change = Schemas["Change"]
type Manifest = Record<string, unknown>

export const engines: Engine[] = ["postgresql", "mongodb"]

export const engineName = (engine: Engine) => (engine === "postgresql" ? "PostgreSQL" : "MongoDB")

/** Server versions the form offers, newest first; the first is the default. */
export const versions: Record<Engine, { label: string; image: string }[]> = {
  postgresql: [
    { label: "18", image: "ghcr.io/cloudnative-pg/postgresql:18-standard-trixie" },
    { label: "17", image: "ghcr.io/cloudnative-pg/postgresql:17-standard-trixie" },
    { label: "16", image: "ghcr.io/cloudnative-pg/postgresql:16-standard-trixie" },
  ],
  mongodb: [
    { label: "8.0", image: "percona/percona-server-mongodb:8.0.32-14" },
    { label: "7.0", image: "percona/percona-server-mongodb:7.0.43-23" },
  ],
}

/** The Percona Backup for MongoDB agent the operator's 1.23 chart ships. */
const PBM_IMAGE = "percona/percona-backup-mongodb:2.15.0"

export type SizePreset = "small" | "medium" | "large"
export const sizePresets: SizePreset[] = ["small", "medium", "large"]

interface Resources {
  requests: { cpu: string; memory: string }
  limits: { memory: string }
}

/** Per member. MongoDB's cache takes half of memory less 1 GiB, at least 256 MiB, so it starts
 * higher. https://www.mongodb.com/docs/manual/core/wiredtiger/#memory-use */
export const presetResources: Record<Engine, Record<SizePreset, Resources>> = {
  postgresql: {
    small: { requests: { cpu: "100m", memory: "256Mi" }, limits: { memory: "512Mi" } },
    medium: { requests: { cpu: "500m", memory: "1Gi" }, limits: { memory: "1Gi" } },
    large: { requests: { cpu: "1", memory: "2Gi" }, limits: { memory: "2Gi" } },
  },
  mongodb: {
    small: { requests: { cpu: "100m", memory: "512Mi" }, limits: { memory: "1Gi" } },
    medium: { requests: { cpu: "500m", memory: "1Gi" }, limits: { memory: "2Gi" } },
    large: { requests: { cpu: "1", memory: "2Gi" }, limits: { memory: "4Gi" } },
  },
}

/** The preset whose numbers the database asks for, if any. */
export function presetOf(engine: Engine, resources: unknown): SizePreset | undefined {
  const asked = sorted(resources ?? null)
  return sizePresets.find((p) => sorted(presetResources[engine][p]) === asked)
}

/** JSON with object keys in order: the API may return them in another. */
const sorted = (value: unknown): string =>
  JSON.stringify(value, (_, v: unknown) =>
    v && typeof v === "object" && !Array.isArray(v)
      ? Object.fromEntries(Object.entries(v).sort(([a], [b]) => a.localeCompare(b)))
      : v,
  )

export interface NewDatabase {
  engine: Engine
  name: string
  /** A `versions` label. */
  version: string
  members: number
  storage: string
  /** The StorageClass its volumes ask for; none for the cluster's default. */
  storageClass?: string
  size: SizePreset
}

/** `version`, or the newest version the kernels allow when they refuse it. */
export function allowedVersion(engine: Engine, version: string, blocked: string[]): string {
  if (!blocked.includes(version)) return version
  return versions[engine].find((v) => !blocked.includes(v.label))?.label ?? version
}

/** Defaults for the form: one member on a single node, else three. */
/** Versions that do not start on these kernels: MongoDB 8 refuses Linux 6.19 and newer
 * (https://jira.mongodb.org/browse/SERVER-121912). */
export function blockedVersions(engine: Engine, kernels: string[]): string[] {
  if (engine !== "mongodb") return []
  const tooNew = kernels.some((kernel) => {
    const [major = 0, minor = 0] = kernel.split(/[.-]/).map(Number)
    return major > 6 || (major === 6 && minor >= 19)
  })
  return tooNew ? versions.mongodb.filter((v) => v.label.startsWith("8.")).map((v) => v.label) : []
}

export function newDatabase(engine: Engine, nodes: number, blocked: string[] = []): NewDatabase {
  return {
    engine,
    name: engine === "postgresql" ? "postgres" : "mongodb",
    version: versions[engine].find((v) => !blocked.includes(v.label))?.label ?? "",
    members: engine === "postgresql" || nodes <= 1 ? 1 : 3,
    storage: engine === "postgresql" ? "5Gi" : "10Gi",
    size: "small",
  }
}

/** Member counts the form offers: PostgreSQL 1 to 5; a MongoDB replica set has an odd count. */
export const memberChoices: Record<Engine, number[]> = {
  postgresql: [1, 2, 3, 4, 5],
  mongodb: [1, 3, 5],
}

const DNS_LABEL = /^[a-z]([-a-z0-9]*[a-z0-9])?$/
/** Short enough for every name the operators derive from it (`<name>-rs0-0`, `<name>-backups`). */
const NAME_MAX = 30

export type NameProblem = "empty" | "invalid" | "long" | "taken"

export function nameProblem(name: string, taken: string[]): NameProblem | undefined {
  if (name === "") return "empty"
  if (name.length > NAME_MAX) return "long"
  if (!DNS_LABEL.test(name)) return "invalid"
  if (taken.includes(name)) return "taken"
  return undefined
}

const create = (object: Manifest): Change => ({
  op: "create",
  object: object as Record<string, never>,
})

const imageOf = (engine: Engine, label: string) =>
  versions[engine].find((v) => v.label === label)?.image ?? versions[engine][0]?.image ?? ""

/** The object the form creates. */
function databaseObject(form: NewDatabase): Manifest {
  const resources = presetResources[form.engine][form.size]
  if (form.engine === "postgresql")
    return {
      apiVersion: "postgresql.cnpg.io/v1",
      kind: "Cluster",
      metadata: { name: form.name },
      spec: {
        instances: form.members,
        imageName: imageOf("postgresql", form.version),
        storage: {
          size: form.storage,
          ...(form.storageClass ? { storageClass: form.storageClass } : {}),
        },
        resources,
      },
    }
  return {
    apiVersion: "psmdb.percona.com/v1",
    kind: "PerconaServerMongoDB",
    // The operator deletes the data claims with the cluster, as CloudNativePG does.
    // https://docs.percona.com/percona-operator-for-mongodb/operator.html#metadata
    metadata: { name: form.name, finalizers: ["percona.com/delete-psmdb-pvc"] },
    spec: {
      image: imageOf("mongodb", form.version),
      // Fewer than three members is allowed only when asked for by name: it has no redundancy.
      // https://docs.percona.com/percona-operator-for-mongodb/operator.html#unsafeflagsreplsetsize
      ...(form.members < 3 ? { unsafeFlags: { replsetSize: true } } : {}),
      replsets: [
        {
          name: "rs0",
          size: form.members,
          resources,
          volumeSpec: {
            persistentVolumeClaim: {
              ...(form.storageClass ? { storageClassName: form.storageClass } : {}),
              resources: { requests: { storage: form.storage } },
            },
          },
        },
      ],
      sharding: { enabled: false },
      // The user services connect as; the operator keeps its password in
      // `<name>-custom-user-secret`, key `app`.
      users: [{ name: "app", db: "app", roles: [{ name: "readWrite", db: "app" }] }],
      backup: { enabled: false, image: PBM_IMAGE },
    },
  }
}

export const createDatabase = (form: NewDatabase): Change[] => [create(databaseObject(form))]

export type Frequency = "daily" | "weekly"

export interface BackupTarget {
  /** `https://…`, the S3-compatible service. */
  endpoint: string
  bucket: string
  /** A folder in the bucket; the database's name when left empty. */
  path: string
  region: string
  accessKey: string
  secretKey: string
  frequency: Frequency
  /** How many days of backups to keep. */
  keepDays: number
}

export const newBackupTarget = (): BackupTarget => ({
  endpoint: "",
  bucket: "",
  path: "",
  region: "",
  accessKey: "",
  secretKey: "",
  frequency: "daily",
  keepDays: 7,
})

export type TargetProblem = "endpoint" | "bucket" | "keys" | "keep"

export function targetProblems(target: BackupTarget): TargetProblem[] {
  const problems: TargetProblem[] = []
  if (!/^https?:\/\/[^\s/]+/.test(target.endpoint.trim())) problems.push("endpoint")
  if (!/^[a-z0-9][a-z0-9.-]{1,61}[a-z0-9]$/.test(target.bucket.trim())) problems.push("bucket")
  if (target.accessKey.trim() === "" || target.secretKey.trim() === "") problems.push("keys")
  if (!Number.isInteger(target.keepDays) || target.keepDays < 1 || target.keepDays > 365)
    problems.push("keep")
  return problems
}

/** The names Zeabur gives what backs a database up. */
const backupNames = (name: string) => ({
  secret: `${name}-backups-s3`,
  store: `${name}-backups`,
  schedule: `${name}-scheduled`,
})

const folder = (target: BackupTarget, name: string) =>
  target.path.trim().replace(/^\/+|\/+$/g, "") || name

/** CloudNativePG's cron has a seconds field first; Percona's has five fields. 03:00 UTC. */
const cron = (frequency: Frequency, seconds: boolean) =>
  `${seconds ? "0 " : ""}0 3 * * ${frequency === "weekly" ? "0" : "*"}`

const PLUGIN = "barman-cloud.cloudnative-pg.io"

/**
 * The writes that back a database up: the keys as a Secret, where backups go, the schedule, and
 * the database edited to use them (`object` is the database as written, `Database.object`).
 */
export function enableBackups(database: Database, target: BackupTarget): Change[] {
  const names = backupNames(database.name)
  const endpoint = target.endpoint.trim()
  const bucket = target.bucket.trim()
  const region = target.region.trim()
  const object = structuredClone(database.object) as Manifest & { spec?: Manifest }
  const spec = (object.spec ??= {})
  const { group, version, resource } = databaseAddress(database)
  const edit: Change = {
    op: "edit",
    resource: { group, version, resource },
    name: database.name,
    yaml: "",
  }
  if (database.engine === "postgresql") {
    const secret = create({
      apiVersion: "v1",
      kind: "Secret",
      metadata: { name: names.secret },
      type: "Opaque",
      stringData: {
        ACCESS_KEY_ID: target.accessKey.trim(),
        ACCESS_SECRET_KEY: target.secretKey.trim(),
        ...(region ? { REGION: region } : {}),
      },
    })
    const store = create({
      apiVersion: "barmancloud.cnpg.io/v1",
      kind: "ObjectStore",
      metadata: { name: names.store },
      spec: {
        retentionPolicy: `${target.keepDays}d`,
        configuration: {
          destinationPath: `s3://${bucket}/${folder(target, database.name)}`,
          endpointURL: endpoint,
          s3Credentials: {
            accessKeyId: { name: names.secret, key: "ACCESS_KEY_ID" },
            secretAccessKey: { name: names.secret, key: "ACCESS_SECRET_KEY" },
            ...(region ? { region: { name: names.secret, key: "REGION" } } : {}),
          },
          wal: { compression: "gzip" },
          data: { compression: "gzip" },
        },
      },
    })
    const plugins = ((spec.plugins as Manifest[] | undefined) ?? []).filter(
      (p) => p.name !== PLUGIN,
    )
    spec.plugins = [
      ...plugins,
      { name: PLUGIN, isWALArchiver: true, parameters: { barmanObjectName: names.store } },
    ]
    const schedule = create({
      apiVersion: "postgresql.cnpg.io/v1",
      kind: "ScheduledBackup",
      metadata: { name: names.schedule },
      spec: {
        schedule: cron(target.frequency, true),
        backupOwnerReference: "self",
        cluster: { name: database.name },
        method: "plugin",
        pluginConfiguration: { name: PLUGIN },
      },
    })
    return [secret, store, { ...edit, yaml: JSON.stringify(object) }, schedule]
  }
  const secret = create({
    apiVersion: "v1",
    kind: "Secret",
    metadata: { name: names.secret },
    type: "Opaque",
    stringData: {
      AWS_ACCESS_KEY_ID: target.accessKey.trim(),
      AWS_SECRET_ACCESS_KEY: target.secretKey.trim(),
    },
  })
  const backup = (spec.backup as Manifest | undefined) ?? {}
  const runs = target.frequency === "weekly" ? Math.ceil(target.keepDays / 7) : target.keepDays
  spec.backup = {
    ...backup,
    enabled: true,
    image: (backup.image as string | undefined) ?? PBM_IMAGE,
    storages: {
      ...(backup.storages as Manifest | undefined),
      backups: {
        type: "s3",
        s3: {
          bucket,
          prefix: folder(target, database.name),
          endpointUrl: endpoint,
          region: region || "us-east-1",
          credentialsSecret: names.secret,
          forcePathStyle: true,
        },
      },
    },
    tasks: [
      ...((backup.tasks as Manifest[] | undefined) ?? []).filter((t) => t.name !== "scheduled"),
      {
        name: "scheduled",
        enabled: true,
        schedule: cron(target.frequency, false),
        storageName: "backups",
        type: "logical",
        retention: { type: "count", count: runs, deleteFromStorage: true },
      },
    ],
  }
  return [secret, { ...edit, yaml: JSON.stringify(object) }]
}

/** A name for an on-demand backup: the database's, and the time to the second. */
export function backupName(database: string, now: Date) {
  const stamp = now
    .toISOString()
    .replace(/[-:]/g, "")
    .replace(/\.\d+Z$/, "")
    .replace("T", "-")
    .toLowerCase()
  return `${database}-${stamp}`
}

/** One backup, now, to where the database's backups go. */
export function backUpNow(database: Database, now: Date): Change[] {
  const name = backupName(database.name, now)
  if (database.engine === "postgresql")
    return [
      create({
        apiVersion: "postgresql.cnpg.io/v1",
        kind: "Backup",
        metadata: { name },
        spec: {
          cluster: { name: database.name },
          method: "plugin",
          pluginConfiguration: { name: PLUGIN },
        },
      }),
    ]
  return [
    create({
      apiVersion: "psmdb.percona.com/v1",
      kind: "PerconaServerMongoDBBackup",
      metadata: { name },
      spec: {
        clusterName: database.name,
        storageName: database.backups.store?.name ?? "backups",
        type: "logical",
      },
    }),
  ]
}

export const stateHealth: Record<Schemas["DatabaseState"], Health> = {
  ready: "ok",
  progressing: "progress",
  degraded: "warn",
  failed: "fail",
  paused: "idle",
  unknown: "idle",
}

/** Whether backups can be taken now: a target is set and in use. */
export const backupsOn = (database: Database) => database.backups.store?.inUse === true

/** The newest run, if any. */
export const lastRun = (database: Database) => database.backups.runs[0]

/** The connection string, with a placeholder where the password goes. */
export function connectionUri(database: Database, password = "PASSWORD") {
  const c = database.connection
  const user = c.username ? encodeURIComponent(c.username) : "USER"
  const options = c.options ? `?${c.options}` : ""
  const scheme = database.engine === "postgresql" ? "postgresql" : "mongodb"
  return `${scheme}://${user}:${encodeURIComponent(password)}@${c.host}:${c.port}/${c.database ?? ""}${options}`
}

/** Where the database's own object lives in the objects API: a custom resource of its operator. */
export function databaseAddress(database: Database): ObjectAddress {
  const [group = "", version = ""] = database.apiVersion.split("/")
  return { group, version, resource: database.resource, name: database.name }
}

/** The container the server runs in, in each member. */
export const serverContainer: Record<Engine, string> = { postgresql: "postgres", mongodb: "mongod" }

/** The member a shell opens in: PostgreSQL's primary; MongoDB's first member. */
export const shellPod = (database: Database) =>
  database.engine === "postgresql"
    ? (database.primary ?? `${database.name}-1`)
    : `${database.name}-rs0-0`

// The operator mounts its users' Secret into each member at /etc/users-secret.
const mongoLogin =
  'mongosh -u "$(cat /etc/users-secret/MONGODB_DATABASE_ADMIN_USER)" -p "$(cat /etc/users-secret/MONGODB_DATABASE_ADMIN_PASSWORD)" --authenticationDatabase admin'

/** What a terminal runs in the server's container: the engine's own shell, signed in. */
export function shellCommand(database: Database): string[] {
  if (database.engine === "postgresql") return ["psql", database.connection.database ?? "app"]
  return ["bash", "-c", mongoLogin]
}

/** Events about the database's object, its members and their volumes. */
export function eventsOf(events: Event[], database: Database): Event[] {
  const pods = new Set(database.pods.map((p) => p.name))
  const claims = new Set(database.claims)
  return events.filter(({ regarding: r }) => {
    if (!r?.name) return false
    if (r.kind === database.kind) return r.name === database.name
    if (r.kind === "Pod") return pods.has(r.name)
    return r.kind === "PersistentVolumeClaim" && claims.has(r.name)
  })
}

const q = (word: string) => (/^[\w./:=@-]+$/.test(word) ? word : `'${word.replace(/'/g, `'\\''`)}'`)

/** kubectl commands for the tools terminal: the object, and a shell in the database. */
export function commands(project: string, database: Database) {
  const ns = q(project)
  const name = q(database.name)
  const resource = `${database.resource}.${databaseAddress(database).group}`
  const describe = `kubectl get ${resource} ${name} -n ${ns} -o wide`
  const pod = q(shellPod(database))
  const container = serverContainer[database.engine]
  const command =
    database.engine === "postgresql"
      ? shellCommand(database).map(q).join(" ")
      : `bash -c '${mongoLogin}'`
  return { describe, shell: `kubectl exec -it -n ${ns} ${pod} -c ${container} -- ${command}` }
}

export interface DatabaseShape {
  members: number
  /** Unchanged when absent: what the database asks for is no preset. */
  size?: SizePreset
  /** Per member, as `5Gi`. */
  storage: string
}

const BYTES: Record<string, number> = { Mi: 2 ** 20, Gi: 2 ** 30, Ti: 2 ** 40 }

/** Bytes in a size written as `512Mi`, `5Gi` or `1Ti`; `undefined` for anything else. */
function storageBytes(size: string): number | undefined {
  const match = /^(\d+(?:\.\d+)?)(Mi|Gi|Ti)$/.exec(size.trim())
  return match ? Number(match[1]) * (BYTES[match[2] ?? ""] ?? 0) : undefined
}

/** A volume can grow, not shrink. */
export function storageProblem(current: string | null | undefined, next: string) {
  const bytes = storageBytes(next)
  if (bytes === undefined || bytes <= 0) return "invalid" as const
  const now = current ? storageBytes(current) : undefined
  return now !== undefined && bytes < now ? ("shrink" as const) : undefined
}

/** The edit that gives the database `shape`: instances, the size of each, and its storage. A
 * MongoDB volume grows only with `enableVolumeExpansion`, and fewer than three members needs
 * `unsafeFlags.replsetSize`.
 * https://docs.percona.com/percona-operator-for-mongodb/scaling.html */
export function reshapeDatabase(database: Database, shape: DatabaseShape): Change[] {
  const object = structuredClone(database.object) as Manifest & { spec?: Manifest }
  const spec = (object.spec ??= {})
  const resources = shape.size
    ? structuredClone(presetResources[database.engine][shape.size])
    : undefined
  const storage = shape.storage.trim()
  if (database.engine === "postgresql") {
    spec.instances = shape.members
    if (resources) spec.resources = resources
    spec.storage = { ...(spec.storage as Manifest | undefined), size: storage }
  } else {
    const [first = {}, ...rest] = (spec.replsets as Manifest[] | undefined) ?? []
    const volume = (first.volumeSpec as Manifest | undefined) ?? {}
    const claim = (volume.persistentVolumeClaim as Manifest | undefined) ?? {}
    const asked = (claim.resources as Manifest | undefined) ?? {}
    spec.replsets = [
      {
        ...first,
        size: shape.members,
        ...(resources ? { resources } : {}),
        volumeSpec: {
          ...volume,
          persistentVolumeClaim: {
            ...claim,
            resources: {
              ...asked,
              requests: { ...(asked.requests as Manifest | undefined), storage },
            },
          },
        },
      },
      ...rest,
    ]
    if (storage !== database.storage) spec.enableVolumeExpansion = true
    const flags = { ...(spec.unsafeFlags as Manifest | undefined) }
    if (shape.members < 3) flags.replsetSize = true
    else delete flags.replsetSize
    if (Object.keys(flags).length > 0) spec.unsafeFlags = flags
    else delete spec.unsafeFlags
  }
  const { group, version, resource } = databaseAddress(database)
  return [
    {
      op: "edit",
      resource: { group, version, resource },
      name: database.name,
      yaml: JSON.stringify(object),
    },
  ]
}

/** Deleting the database: its object, which the operator deletes its members and volumes with. */
export function deleteDatabase(database: Database): Change[] {
  const { group, version, resource } = databaseAddress(database)
  return [{ op: "delete", resource: { group, version, resource }, name: database.name }]
}
