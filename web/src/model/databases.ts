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
//
// MySQL is a MOCO `MySQLCluster`; its backups are a `BackupPolicy` the cluster names, which MOCO
// runs as a CronJob into `s3://<bucket>/moco/<namespace>/<name>`.
// https://cybozu-go.github.io/moco/backup.html
//
// MariaDB is a mariadb-operator `MariaDB`; its backups are `Backup` objects, scheduled or not.
// https://github.com/mariadb-operator/mariadb-operator/blob/main/docs/logical_backup.md
//
// Dragonfly is a Dragonfly Operator `Dragonfly`; it snapshots to a volume, or to a bucket when
// `spec.snapshot.dir` is `s3://…`.
// https://www.dragonflydb.io/docs/managing-dragonfly/backups
import type { Schemas } from "@/api/client"
import type { ObjectAddress } from "@/api/queries"
import type { Translate } from "@/i18n"
import { AWS_KEYS, keysSecret, needsKeys, type S3Target } from "@/model/s3-destinations"
import type { Event, Health } from "@/model/service"

export type Database = Schemas["Database"]
export type Engine = Schemas["DatabaseEngine"]
type Change = Schemas["Change"]
type Manifest = Record<string, unknown>

export const engines: Engine[] = ["postgresql", "mongodb", "mysql", "mariadb", "dragonfly"]

const ENGINE_NAMES: Record<Engine, string> = {
  postgresql: "PostgreSQL",
  mongodb: "MongoDB",
  mysql: "MySQL",
  mariadb: "MariaDB",
  dragonfly: "Dragonfly",
}

export const engineName = (engine: Engine) => ENGINE_NAMES[engine]

/** The server's licence, when it is not an open-source one. */
export const engineLicence: Partial<Record<Engine, string>> = {
  dragonfly: "Apache-2.0 AND BUSL-1.1",
}

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
  mysql: [{ label: "8.4", image: "ghcr.io/cybozu-go/moco/mysql:8.4.10" }],
  mariadb: [
    { label: "12.3", image: "mariadb:12.3.3" },
    { label: "11.8", image: "mariadb:11.8.3" },
  ],
  dragonfly: [{ label: "1.34", image: "docker.dragonflydb.io/dragonflydb/dragonfly:v1.34.1" }],
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
  mysql: {
    small: { requests: { cpu: "100m", memory: "512Mi" }, limits: { memory: "1Gi" } },
    medium: { requests: { cpu: "500m", memory: "1Gi" }, limits: { memory: "2Gi" } },
    large: { requests: { cpu: "1", memory: "2Gi" }, limits: { memory: "4Gi" } },
  },
  mariadb: {
    small: { requests: { cpu: "100m", memory: "256Mi" }, limits: { memory: "512Mi" } },
    medium: { requests: { cpu: "500m", memory: "1Gi" }, limits: { memory: "1Gi" } },
    large: { requests: { cpu: "1", memory: "2Gi" }, limits: { memory: "2Gi" } },
  },
  dragonfly: {
    small: { requests: { cpu: "100m", memory: "256Mi" }, limits: { memory: "512Mi" } },
    medium: { requests: { cpu: "500m", memory: "1Gi" }, limits: { memory: "1Gi" } },
    large: { requests: { cpu: "1", memory: "2Gi" }, limits: { memory: "2Gi" } },
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

/** Linux 6.19 or later. Ubuntu's `7.0.0-38-generic` is 7.0. */
export function kernel619OrLater(kernel: string): boolean {
  const match = /^\D*(\d+)\.(\d+)/.exec(kernel)
  if (!match) return false
  const [major, minor] = [Number(match[1]), Number(match[2])]
  return major > 6 || (major === 6 && minor >= 19)
}

/** MongoDB 8 or later, from a version label (`8.0`), a version (`8.0.32-14`) or an image. */
function mongo8OrLater(version: string | null | undefined): boolean {
  const match = /(?:^|:)(\d+)\./.exec(version ?? "")
  return match ? Number(match[1]) >= 8 : false
}

// Percona's image sets glibc.pthread.rseq=0 for TCMalloc's per-CPU caches. mongod 8 does not
// start with them on Linux 6.19 and later; with rseq=1 TCMalloc uses per-thread caches.
// https://jira.mongodb.org/browse/SERVER-121912
const TUNABLES = "GLIBC_TUNABLES"
const RSEQ = "glibc.pthread.rseq"
const PER_THREAD = `${RSEQ}=1`

/** `env` with `GLIBC_TUNABLES` set to keep rseq in glibc; other tunables stay. */
function withPerThreadCaches(env: Manifest[] = []): Manifest[] {
  const current = env.find((e) => e.name === TUNABLES)
  if (!current) return [...env, { name: TUNABLES, value: PER_THREAD }]
  const others = typeof current.value === "string" ? current.value.split(":") : []
  const value = [...others.filter((t) => t && !t.startsWith(`${RSEQ}=`)), PER_THREAD].join(":")
  return env.map((e) => (e === current ? { name: TUNABLES, value } : e))
}

const hasPerThreadCaches = (replset: Manifest) =>
  ((replset.env as Manifest[] | undefined) ?? []).some(
    (e) =>
      e.name === TUNABLES && typeof e.value === "string" && e.value.split(":").includes(PER_THREAD),
  )

/** Each replica set of a MongoDB 8 `PerconaServerMongoDB` spec with per-thread caches;
 * `version` counts when the spec names no image. */
export function perThreadCaches(spec: Manifest, version?: string | null) {
  if (!mongo8OrLater((spec.image as string | undefined) ?? version)) return
  const replsets = (spec.replsets as Manifest[] | undefined) ?? []
  spec.replsets = replsets.map((r) =>
    hasPerThreadCaches(r) ? r : { ...r, env: withPerThreadCaches(r.env as Manifest[]) },
  )
}

/** Whether a new database of `form` gets the note: MongoDB 8 where a node runs Linux 6.19+. */
export const kernelNoteForNew = (
  form: Pick<NewDatabase, "engine" | "version">,
  kernels: string[],
) => form.engine === "mongodb" && mongo8OrLater(form.version) && kernels.some(kernel619OrLater)

/** The note a MongoDB 8 gets where a node runs Linux 6.19+: the setting is there, or missing. */
export function kernelNote(database: Database, kernels: string[]): "set" | "missing" | undefined {
  if (database.engine !== "mongodb" || !mongo8OrLater(database.version ?? database.image)) return
  if (!kernels.some(kernel619OrLater)) return
  const spec = (database.object as Manifest & { spec?: Manifest }).spec ?? {}
  const replsets = (spec.replsets as Manifest[] | undefined) ?? []
  return replsets.every(hasPerThreadCaches) ? "set" : "missing"
}

/** The edit that gives an existing MongoDB 8 per-thread caches. */
export function addPerThreadCaches(database: Database): Change[] {
  const object = structuredClone(database.object) as Manifest & { spec?: Manifest }
  perThreadCaches((object.spec ??= {}), database.version ?? database.image)
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

/** Defaults for the form: one member on a single node, else three. */
const defaults: Record<Engine, { name: string; storage: string; quorum: boolean }> = {
  postgresql: { name: "postgres", storage: "5Gi", quorum: false },
  mongodb: { name: "mongodb", storage: "10Gi", quorum: true },
  mysql: { name: "mysql", storage: "10Gi", quorum: true },
  mariadb: { name: "mariadb", storage: "10Gi", quorum: false },
  dragonfly: { name: "dragonfly", storage: "2Gi", quorum: false },
}

export function newDatabase(engine: Engine, nodes: number): NewDatabase {
  const d = defaults[engine]
  return {
    engine,
    name: d.name,
    version: versions[engine][0]?.label ?? "",
    members: d.quorum && nodes > 1 ? 3 : 1,
    storage: d.storage,
    size: "small",
  }
}

/** Member counts the form offers. A MongoDB replica set and a MOCO cluster vote, so their counts
 * are odd. */
export const memberChoices: Record<Engine, number[]> = {
  postgresql: [1, 2, 3, 4, 5],
  mongodb: [1, 3, 5],
  mysql: [1, 3, 5],
  mariadb: [1, 2, 3],
  dragonfly: [1, 2, 3],
}

const DNS_LABEL = /^[a-z]([-a-z0-9]*[a-z0-9])?$/
/** Short enough for every name the operators derive from it (`<name>-rs0-0`, `<name>-backups`,
 * `moco-backup-<name>`). */
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

const restricted = {
  allowPrivilegeEscalation: false,
  capabilities: { drop: ["ALL"] },
}

/** A pod that passes the restricted Pod Security Standard, as `uid`. */
const restrictedPod = (uid: number) => ({
  runAsNonRoot: true,
  runAsUser: uid,
  runAsGroup: uid,
  fsGroup: uid,
  fsGroupChangePolicy: "OnRootMismatch",
  seccompProfile: { type: "RuntimeDefault" },
})

const claimSpec = (storage: string, storageClass?: string) => ({
  accessModes: ["ReadWriteOnce"],
  ...(storageClass ? { storageClassName: storageClass } : {}),
  resources: { requests: { storage } },
})

const ALPHABET = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"

/** 24 letters and digits, safe in a URL without escaping. */
export function newPassword(): string {
  let out = ""
  while (out.length < 24)
    for (const byte of crypto.getRandomValues(new Uint8Array(32)))
      if (byte < 248 && out.length < 24) out += ALPHABET[byte % ALPHABET.length]
  return out
}

const secret = (name: string, data: Record<string, string>) =>
  create({ apiVersion: "v1", kind: "Secret", metadata: { name }, type: "Opaque", stringData: data })

/** MOCO's containers besides `mysqld`, which run as the pod's user. */
const MOCO_CONTAINERS = ["agent", "moco-init", "slow-log"]

type Build = (form: NewDatabase, image: string, resources: Resources) => Manifest

/** The object the form creates, by engine. */
const BUILD: Record<Engine, Build> = {
  postgresql: (form, image, resources) => {
    return {
      apiVersion: "postgresql.cnpg.io/v1",
      kind: "Cluster",
      metadata: { name: form.name },
      spec: {
        instances: form.members,
        imageName: image,
        storage: {
          size: form.storage,
          ...(form.storageClass ? { storageClass: form.storageClass } : {}),
        },
        resources,
      },
    }
  },
  mongodb: (form, image, resources) => {
    const object = {
      apiVersion: "psmdb.percona.com/v1",
      kind: "PerconaServerMongoDB",
      // The operator deletes the data claims with the cluster, as CloudNativePG does.
      // https://docs.percona.com/percona-operator-for-mongodb/operator.html#metadata
      metadata: { name: form.name, finalizers: ["percona.com/delete-psmdb-pvc"] },
      spec: {
        image,
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
    perThreadCaches(object.spec)
    return object
  },
  mysql: (form, image, resources) => {
    // MOCO's images run as 10000. The annotations name the database and Secret that the
    // setup Job makes for services.
    return {
      apiVersion: "moco.cybozu.com/v1beta2",
      kind: "MySQLCluster",
      metadata: {
        name: form.name,
        annotations: { [APP_DATABASE]: "app", [APP_SECRET]: `${form.name}-app` },
      },
      spec: {
        replicas: form.members,
        podTemplate: {
          spec: {
            securityContext: restrictedPod(10000),
            containers: [{ name: "mysqld", image, resources, securityContext: restricted }],
          },
        },
        volumeClaimTemplates: [
          {
            metadata: { name: "mysql-data" },
            spec: claimSpec(form.storage, form.storageClass),
          },
        ],
        overwriteContainers: MOCO_CONTAINERS.map((name) => ({
          name,
          securityContext: restricted,
        })),
      },
    }
  },
  mariadb: (form, image, resources) => {
    return {
      apiVersion: "k8s.mariadb.com/v1alpha1",
      kind: "MariaDB",
      metadata: { name: form.name },
      spec: {
        image,
        replicas: form.members,
        ...(form.members > 1 ? { replication: { enabled: true } } : {}),
        rootPasswordSecretKeyRef: { name: `${form.name}-root`, key: "password", generate: false },
        database: "app",
        username: "app",
        passwordSecretKeyRef: { name: `${form.name}-app`, key: "password", generate: false },
        storage: {
          size: form.storage,
          ...(form.storageClass ? { storageClassName: form.storageClass } : {}),
        },
        resources,
        podSecurityContext: restrictedPod(999),
        securityContext: restricted,
      },
    }
  },
  dragonfly: (form, image, resources) => {
    return {
      apiVersion: "dragonflydb.io/v1alpha1",
      kind: "Dragonfly",
      metadata: { name: form.name },
      spec: {
        image,
        replicas: form.members,
        resources,
        authentication: { passwordFromSecret: { name: `${form.name}-auth`, key: "password" } },
        // Snapshots on a volume each hour; Dragonfly also saves one when it stops.
        snapshot: {
          cron: "0 * * * *",
          persistentVolumeClaimSpec: claimSpec(form.storage, form.storageClass),
        },
        podSecurityContext: restrictedPod(999),
        containerSecurityContext: restricted,
      },
    }
  },
}

const databaseObject = (form: NewDatabase): Manifest =>
  BUILD[form.engine](
    form,
    imageOf(form.engine, form.version),
    presetResources[form.engine][form.size],
  )

/** The Secrets Zeabur makes for a database: the app passwords, MariaDB's root, Dragonfly's. */
function passwordSecrets(form: NewDatabase, password: () => string): Change[] {
  if (form.engine === "mysql") return [secret(`${form.name}-app`, { password: password() })]
  if (form.engine === "mariadb")
    return [
      secret(`${form.name}-root`, { password: password() }),
      secret(`${form.name}-app`, { password: password() }),
    ]
  if (form.engine === "dragonfly") return [secret(`${form.name}-auth`, { password: password() })]
  return []
}

export const APP_DATABASE = "zeabur.com/app-database"
export const APP_SECRET = "zeabur.com/app-secret"

// Waits for the primary, then makes the database and its user as `moco-writable`. The passwords
// go in through a file descriptor and standard input, so no process's arguments hold them.
const APP_SETUP = `set -eu
sql() { mysql --defaults-extra-file=<(printf '[client]\\npassword=%s\\n' "$WRITABLE_PASSWORD") -h "$HOST" -u moco-writable "$@"; }
for _ in $(seq 120); do sql -e 'SELECT 1' >/dev/null 2>&1 && break; sleep 5; done
sql <<SQL
CREATE DATABASE IF NOT EXISTS app;
CREATE USER IF NOT EXISTS 'app'@'%' IDENTIFIED BY '$APP_PASSWORD';
GRANT ALL ON app.* TO 'app'@'%';
SQL`

/** MOCO makes no database: this Job makes `app` and its user, as the person who creates it. */
function appSetup(form: NewDatabase, image: string): Manifest {
  const from = (name: string, key: string) => ({ secretKeyRef: { name, key } })
  return {
    apiVersion: "batch/v1",
    kind: "Job",
    metadata: { name: `${form.name}-app-setup` },
    spec: {
      backoffLimit: 6,
      template: {
        spec: {
          restartPolicy: "OnFailure",
          securityContext: restrictedPod(10000),
          containers: [
            {
              name: "setup",
              image,
              command: ["bash", "-c", APP_SETUP],
              env: [
                { name: "HOST", value: `moco-${form.name}-primary` },
                {
                  name: "WRITABLE_PASSWORD",
                  valueFrom: from(`moco-${form.name}`, "WRITABLE_PASSWORD"),
                },
                { name: "APP_PASSWORD", valueFrom: from(`${form.name}-app`, "password") },
              ],
              securityContext: restricted,
            },
          ],
        },
      },
    },
  }
}

export const createDatabase = (form: NewDatabase, password = newPassword): Change[] => [
  ...passwordSecrets(form, password),
  create(databaseObject(form)),
  ...(form.engine === "mysql" ? [create(appSetup(form, imageOf("mysql", form.version)))] : []),
]

export type Frequency = "daily" | "weekly"

export interface BackupTarget extends S3Target {
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
  if (needsKeys(target)) problems.push("keys")
  if (!Number.isInteger(target.keepDays) || target.keepDays < 1 || target.keepDays > 365)
    problems.push("keep")
  return problems
}

/** The names Zeabur gives what backs a database up. */
export const backupNames = (name: string) => ({
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

/** The bucket and folder of `s3://bucket/folder`. */
export function splitDestination(destination: string | null | undefined) {
  const [bucket = "", ...rest] = (destination ?? "").replace(/^s3:\/\//, "").split("/")
  return { bucket, folder: rest.join("/") }
}

/** MOCO's backup and restore Jobs: the bucket, the keys from `secretName` as AWS variables, and a
 * scratch volume as big as the data. They run as the namespace's `default` ServiceAccount, as
 * MOCO's Jobs call no Kubernetes API.
 * https://cybozu-go.github.io/moco/backup.html */
function mocoJobConfig(
  database: Pick<Database, "storage" | "storageClass">,
  bucket: string,
  endpoint: string | null | undefined,
  secretName: string,
) {
  return {
    serviceAccountName: "default",
    bucketConfig: {
      bucketName: bucket,
      ...(endpoint ? { endpointURL: endpoint } : {}),
      usePathStyle: true,
    },
    envFrom: [{ secretRef: { name: secretName } }],
    workVolume: {
      ephemeral: {
        volumeClaimTemplate: {
          spec: claimSpec(database.storage ?? "10Gi", database.storageClass ?? undefined),
        },
      },
    },
  }
}

/** mariadb-operator's S3 storage: the endpoint without its scheme, TLS from it. */
function mariaStorage(
  bucket: string,
  prefix: string,
  endpoint: string,
  region: string,
  secretName: string,
) {
  return {
    s3: {
      bucket,
      prefix,
      endpoint: endpoint.replace(/^https?:\/\//, "").replace(/\/+$/, ""),
      region: region || "us-east-1",
      accessKeyIdSecretKeyRef: { name: secretName, key: "AWS_ACCESS_KEY_ID" },
      secretAccessKeySecretKeyRef: { name: secretName, key: "AWS_SECRET_ACCESS_KEY" },
      tls: { enabled: endpoint.startsWith("https://") },
    },
  }
}

const awsKeys = (name: string, target: BackupTarget, region: string) =>
  keysSecret(name, target, AWS_KEYS, { AWS_REGION: region || "us-east-1" })

/** Engines whose backups have no retention: the person sets a lifecycle rule on the bucket. */
export const keepsEverything = (engine: Engine) => engine === "mysql" || engine === "dragonfly"

/** MOCO writes to a fixed folder, `moco/<namespace>/<name>`. */
export const fixedFolder = (engine: Engine) => engine === "mysql"

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
  const edit = (): Change => ({
    op: "edit",
    resource: { group, version, resource },
    name: database.name,
    yaml: JSON.stringify(object),
  })
  switch (database.engine) {
    case "postgresql": {
      const keys = keysSecret(
        names.secret,
        target,
        { accessKeyId: "ACCESS_KEY_ID", secretAccessKey: "ACCESS_SECRET_KEY" },
        region ? { REGION: region } : {},
      )
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
      return [keys, store, edit(), schedule]
    }
    case "mongodb": {
      const keys = keysSecret(names.secret, target, AWS_KEYS)
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
      return [keys, edit()]
    }
    case "mysql": {
      const keys = awsKeys(names.secret, target, region)
      const policy = create({
        apiVersion: "moco.cybozu.com/v1beta2",
        kind: "BackupPolicy",
        metadata: { name: names.store },
        spec: {
          schedule: cron(target.frequency, false),
          concurrencyPolicy: "Forbid",
          jobConfig: mocoJobConfig(database, bucket, endpoint, names.secret),
        },
      })
      spec.backupPolicyName = names.store
      return [keys, policy, edit()]
    }
    case "mariadb": {
      const keys = awsKeys(names.secret, target, region)
      const backup = create({
        apiVersion: "k8s.mariadb.com/v1alpha1",
        kind: "Backup",
        metadata: { name: names.schedule },
        spec: {
          mariaDbRef: { name: database.name },
          schedule: { cron: cron(target.frequency, false) },
          maxRetention: `${target.keepDays * 24}h`,
          compression: "gzip",
          storage: mariaStorage(
            bucket,
            folder(target, database.name),
            endpoint,
            region,
            names.secret,
          ),
          podSecurityContext: restrictedPod(999),
          securityContext: restricted,
        },
      })
      return [keys, backup]
    }
    case "dragonfly": {
      const keys = awsKeys(names.secret, target, region)
      const https = !endpoint.startsWith("http://")
      const host = endpoint.replace(/^https?:\/\//, "").replace(/\/+$/, "")
      const own = (arg: string) =>
        arg.startsWith("--s3_endpoint=") || arg.startsWith("--s3_use_https=")
      spec.args = [
        ...((spec.args as string[] | undefined) ?? []).filter((arg) => !own(arg)),
        `--s3_endpoint=${host}`,
        `--s3_use_https=${https}`,
      ]
      const key = (name: string) => ({
        name,
        valueFrom: { secretKeyRef: { name: names.secret, key: name } },
      })
      spec.env = [
        ...((spec.env as Manifest[] | undefined) ?? []).filter(
          (e) => !String(e.name).startsWith("AWS_"),
        ),
        key("AWS_ACCESS_KEY_ID"),
        key("AWS_SECRET_ACCESS_KEY"),
        key("AWS_REGION"),
      ]
      spec.snapshot = {
        ...(spec.snapshot as Manifest | undefined),
        dir: `s3://${bucket}/${folder(target, database.name)}`,
        cron: cron(target.frequency, false),
      }
      return [keys, edit()]
    }
  }
}

/** Engines whose database restarts once its backups are turned on: a sidecar or new arguments. */
const restartsForBackups = (engine: Engine) =>
  engine === "postgresql" || engine === "mongodb" || engine === "dragonfly"

/** What `enableBackups` does, in the form's words. */
export function backupsSummary(t: Translate, database: Database, target: BackupTarget) {
  const names = backupNames(database.name)
  const namespace = database.origin.namespace ?? ""
  const path = fixedFolder(database.engine)
    ? `moco/${namespace}/${database.name}`
    : folder(target, database.name)
  const where = `s3://${target.bucket.trim()}/${path} (${target.endpoint.trim()})`
  return {
    lines: [
      t("db.backup.said", database.name, target.frequency === "weekly", where),
      keepsEverything(database.engine)
        ? t("db.backup.keepsEverything")
        : t("db.backup.saidKeep", target.keepDays),
      target.destination === undefined
        ? t("db.backup.saidKeys", names.secret)
        : t("confirm.copyS3Keys", names.secret, target.destination),
      ...(restartsForBackups(database.engine) ? [t("db.backup.saidRestart", database.name)] : []),
    ],
    action: t("db.backup.setUp"),
  }
}

/** What `backUpNow` does, in the form's words. */
export function backUpNowSummary(t: Translate, database: Database) {
  const destination = database.backups.store?.destination
  return {
    lines: [
      destination
        ? t("db.backup.saidNow", database.name, destination)
        : t("db.backup.planNow", database.name),
    ],
    action: t("db.backup.now"),
  }
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

/** Engines that take a backup on demand: MOCO and Dragonfly only back up on their schedule. */
export const canBackUpNow = (engine: Engine) =>
  engine === "postgresql" || engine === "mongodb" || engine === "mariadb"

/** One backup, now, to where the database's backups go. */
export function backUpNow(database: Database, now: Date): Change[] {
  const name = backupName(database.name, now)
  const store = database.backups.store
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
  if (database.engine === "mariadb") {
    const { bucket, folder: prefix } = splitDestination(store?.destination)
    return [
      create({
        apiVersion: "k8s.mariadb.com/v1alpha1",
        kind: "Backup",
        metadata: { name },
        spec: {
          mariaDbRef: { name: database.name },
          compression: "gzip",
          storage: mariaStorage(
            bucket,
            prefix,
            store?.endpoint ?? "",
            store?.region ?? "",
            store?.secret ?? backupNames(database.name).secret,
          ),
          podSecurityContext: restrictedPod(999),
          securityContext: restricted,
        },
      }),
    ]
  }
  return [
    create({
      apiVersion: "psmdb.percona.com/v1",
      kind: "PerconaServerMongoDBBackup",
      metadata: { name },
      spec: {
        clusterName: database.name,
        storageName: store?.name ?? "backups",
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

const SCHEMES: Record<Engine, string> = {
  postgresql: "postgresql",
  mongodb: "mongodb",
  mysql: "mysql",
  mariadb: "mysql",
  dragonfly: "redis",
}

/** The connection string, with a placeholder where the password goes. */
export function connectionUri(database: Database, password = "PASSWORD") {
  const c = database.connection
  const scheme = SCHEMES[database.engine]
  if (database.engine === "dragonfly")
    return `${scheme}://:${encodeURIComponent(password)}@${c.host}:${c.port}`
  const user = c.username ? encodeURIComponent(c.username) : "USER"
  const options = c.options ? `?${c.options}` : ""
  return `${scheme}://${user}:${encodeURIComponent(password)}@${c.host}:${c.port}/${c.database ?? ""}${options}`
}

/** Where the database's own object lives in the objects API: a custom resource of its operator. */
export function databaseAddress(database: Database): ObjectAddress {
  const [group = "", version = ""] = database.apiVersion.split("/")
  return { group, version, resource: database.resource, name: database.name }
}

/** The tool an export runs in the primary; Dragonfly has none. */
export const exportTool: Partial<Record<Engine, string>> = {
  postgresql: "pg_dump",
  mongodb: "mongodump",
  mysql: "mysqldump",
  mariadb: "mariadb-dump",
}

/** Where the dump downloads from: the server runs the tool through the person's `pods/exec`. */
export const exportUrl = (project: string, database: Pick<Database, "engine" | "name">) =>
  `/api/v1/projects/${encodeURIComponent(project)}/databases/${database.engine}/${encodeURIComponent(database.name)}/export`

/** The container the server runs in, in each member. */
export const serverContainer: Record<Engine, string> = {
  postgresql: "postgres",
  mongodb: "mongod",
  mysql: "mysqld",
  mariadb: "mariadb",
  dragonfly: "dragonfly",
}

/** The member a shell opens in: the primary when known, else the first member. */
export function shellPod(database: Database): string {
  switch (database.engine) {
    case "postgresql":
      return database.primary ?? `${database.name}-1`
    case "mongodb":
      return `${database.name}-rs0-0`
    case "mysql":
      return database.primary ?? `moco-${database.name}-0`
    case "mariadb":
    case "dragonfly":
      return database.primary ?? `${database.name}-0`
  }
}

// The operator mounts its users' Secret into each member at /etc/users-secret.
const mongoLogin =
  'mongosh -u "$(cat /etc/users-secret/MONGODB_DATABASE_ADMIN_USER)" -p "$(cat /etc/users-secret/MONGODB_DATABASE_ADMIN_PASSWORD)" --authenticationDatabase admin'

/** What a terminal runs in the server's container: the engine's own shell, signed in where the
 * container holds a password. MOCO's does not, so `mysql` asks for `moco-writable`'s. */
export function shellCommand(database: Database): string[] {
  switch (database.engine) {
    case "postgresql":
      return ["psql", database.connection.database ?? "app"]
    case "mongodb":
      return ["bash", "-c", mongoLogin]
    case "mysql":
      return ["mysql", "-h", "127.0.0.1", "-u", "moco-writable", "-p"]
    case "mariadb":
      return ["bash", "-c", 'MYSQL_PWD="$MARIADB_ROOT_PASSWORD" exec mariadb -u root']
    case "dragonfly":
      return ["sh", "-c", 'REDISCLI_AUTH="$DFLY_requirepass" exec redis-cli']
  }
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
  const command = shellCommand(database).map(q).join(" ")
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

/** Engines whose volumes do not grow from the database's object: the Dragonfly Operator keeps
 * a StatefulSet's claim template, which Kubernetes does not change. */
export const storageFixed = (engine: Engine) => engine === "dragonfly"

/** Patch `container` in `containers` by name, adding it when absent. */
function withContainer(containers: Manifest[] | undefined, name: string, patch: Manifest) {
  const list = containers ?? []
  return list.some((c) => c.name === name)
    ? list.map((c) => (c.name === name ? { ...c, ...patch } : c))
    : [...list, { name, ...patch }]
}

interface Reshaping {
  spec: Manifest
  members: number
  resources?: Resources
  storage: string
  database: Database
}

const field = (spec: Manifest, key: string) => (spec[key] as Manifest | undefined) ?? {}

function reshapeMongodb({ spec, members, resources, storage, database }: Reshaping) {
  const [first = {}, ...rest] = (spec.replsets as Manifest[] | undefined) ?? []
  const volume = field(first, "volumeSpec")
  const claim = field(volume, "persistentVolumeClaim")
  spec.replsets = [
    {
      ...first,
      size: members,
      ...(resources ? { resources } : {}),
      volumeSpec: { ...volume, persistentVolumeClaim: withStorage(claim, storage) },
    },
    ...rest,
  ]
  if (storage !== database.storage) spec.enableVolumeExpansion = true
  const { replsetSize: _, ...flags } = field(spec, "unsafeFlags")
  const kept = members < 3 ? { ...flags, replsetSize: true } : flags
  if (Object.keys(kept).length > 0) spec.unsafeFlags = kept
  else delete spec.unsafeFlags
}

function reshapeMysql({ spec, members, resources, storage }: Reshaping) {
  spec.replicas = members
  const template = field(spec, "podTemplate")
  const pod = field(template, "spec")
  if (resources) {
    const containers = withContainer(pod.containers as Manifest[] | undefined, "mysqld", {
      resources,
    })
    spec.podTemplate = { ...template, spec: { ...pod, containers } }
  }
  const templates = (spec.volumeClaimTemplates as Manifest[] | undefined) ?? []
  spec.volumeClaimTemplates = templates.map((t) =>
    field(t, "metadata").name === "mysql-data"
      ? { ...t, spec: withStorage(field(t, "spec"), storage) }
      : t,
  )
}

const RESHAPE: Record<Engine, (shaping: Reshaping) => void> = {
  postgresql: ({ spec, members, resources, storage }) => {
    spec.instances = members
    if (resources) spec.resources = resources
    spec.storage = { ...field(spec, "storage"), size: storage }
  },
  mongodb: reshapeMongodb,
  mysql: reshapeMysql,
  mariadb: ({ spec, members, resources, storage }) => {
    spec.replicas = members
    if (members > 1) spec.replication = { ...field(spec, "replication"), enabled: true }
    if (resources) spec.resources = resources
    spec.storage = { ...field(spec, "storage"), size: storage }
  },
  dragonfly: ({ spec, members, resources }) => {
    spec.replicas = members
    if (resources) spec.resources = resources
  },
}

/** The edit that gives the database `shape`: instances, the size of each, and its storage. A
 * MongoDB volume grows only with `enableVolumeExpansion`, and fewer than three members needs
 * `unsafeFlags.replsetSize`; a MariaDB of more than one member replicates.
 * https://docs.percona.com/percona-operator-for-mongodb/scaling.html */
export function reshapeDatabase(database: Database, shape: DatabaseShape): Change[] {
  const object = structuredClone(database.object) as Manifest & { spec?: Manifest }
  RESHAPE[database.engine]({
    spec: (object.spec ??= {}),
    members: shape.members,
    resources: shape.size
      ? structuredClone(presetResources[database.engine][shape.size])
      : undefined,
    storage: shape.storage.trim(),
    database,
  })
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

/** A claim spec asking for `storage`. */
function withStorage(claim: Manifest, storage: string): Manifest {
  const asked = (claim.resources as Manifest | undefined) ?? {}
  return {
    ...claim,
    resources: { ...asked, requests: { ...(asked.requests as Manifest | undefined), storage } },
  }
}

/** Deleting the database: its object, which the operator deletes its members and volumes with. */
export function deleteDatabase(database: Database): Change[] {
  const { group, version, resource } = databaseAddress(database)
  return [{ op: "delete", resource: { group, version, resource }, name: database.name }]
}
