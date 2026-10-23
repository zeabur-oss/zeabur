// Restoring a backup into a new database, and moving services between the two. The new database
// copies the original's spec and starts from the backup; the original is not changed. The
// `zeabur.com/restored-*` annotations tie the two together for the pages.
import type { Schemas } from "@/api/client"
import { databaseOfEnv, type Target } from "@/model/connect"
import {
  APP_DATABASE,
  APP_SECRET,
  newPassword,
  splitDestination,
  type Database,
} from "@/model/databases"

type Change = Schemas["Change"]
type EnvEdit = Schemas["EnvEdit"]
type Manifest = Record<string, unknown>

/** A backup a new database can start from. */
export interface RestorePoint {
  /** The run's name; `latest` for Dragonfly's newest snapshot. */
  backup: string
  /** When the backup finished, if known. */
  time?: string
  /** What the operator restores: CloudNativePG's backup ID, Percona's destination, a time. */
  from?: string
}

/** The backups a database can be restored from, newest first. Dragonfly keeps one snapshot set
 * per save and loads the newest. */
export function restorePoints(database: Database): RestorePoint[] {
  if (database.engine === "dragonfly")
    return database.backups.store?.inUse ? [{ backup: "latest" }] : []
  return database.backups.runs
    .filter((r) => r.outcome === "succeeded" && r.restoreFrom)
    .map((r) => ({
      backup: r.name,
      time: r.finished ?? r.started ?? undefined,
      from: r.restoreFrom ?? undefined,
    }))
}

const NAME_MAX = 30

/** `<name>-restored-<MMDD>`, the original's name cut to fit. */
export function restoredName(name: string, now: Date): string {
  const suffix = `-restored-${String(now.getMonth() + 1).padStart(2, "0")}${String(now.getDate()).padStart(2, "0")}`
  return `${name.slice(0, NAME_MAX - suffix.length).replace(/-+$/, "")}${suffix}`
}

const PLUGIN = "barman-cloud.cloudnative-pg.io"
/** Copies the newest snapshot set into the new folder before Dragonfly starts. */
const AWS_CLI = "amazon/aws-cli:2.27.0"

// The newest `dump-<time>-summary.dfs` names its set; the copy runs once, into an empty folder.
const COPY_SNAPSHOT = `set -eu
endpoint=\${ENDPOINT:+--endpoint-url=$ENDPOINT}
if [ -n "$(aws s3 ls $endpoint "$TO/")" ]; then exit 0; fi
latest=$(aws s3 ls $endpoint "$FROM/" | awk '{print $4}' | grep -- '-summary.dfs$' | sort | tail -n 1)
if [ -z "$latest" ]; then exit 0; fi
aws s3 cp $endpoint --recursive --exclude '*' --include "\${latest%-summary.dfs}-*" "$FROM/" "$TO/"`

type Spec = Manifest
type Store = NonNullable<Database["backups"]["store"]>

const create = (body: Manifest): Change => ({ op: "create", object: body as Record<string, never> })

/** What one engine adds to the copy: it edits `spec` in place and returns the writes. */
interface Restoring {
  source: Database
  point: RestorePoint
  name: string
  project: string
  spec: Spec
  store?: Store
  /** The copy's object, once `spec` is set. */
  object: Manifest
  /** A new Secret with a password, key `password`. */
  secret: (name: string) => Change
}

function restorePostgresql({ source, point, spec, store, object }: Restoring): Change[] {
  delete spec.plugins
  spec.bootstrap = {
    recovery: {
      source: source.name,
      database: source.app?.database ?? source.connection.database ?? "app",
      owner: source.app?.username ?? "app",
      recoveryTarget: { backupID: point.from, targetImmediate: true },
    },
  }
  spec.externalClusters = [
    {
      name: source.name,
      plugin: {
        name: PLUGIN,
        parameters: { barmanObjectName: store?.name, serverName: source.name },
      },
    },
  ]
  return [create(object)]
}

// A logical dump holds the system users, so the copy keeps the original's users Secret.
function restoreMongodb({ source, point, name, spec, store, object }: Restoring): Change[] {
  const backup = (spec.backup as Manifest | undefined) ?? {}
  const storages = (backup.storages as Record<string, Manifest> | undefined) ?? {}
  const s3 = storages[store?.name ?? "backups"]?.s3
  const secrets = (spec.secrets as Manifest | undefined) ?? {}
  spec.backup = { ...backup, tasks: [] }
  spec.secrets = { ...secrets, users: secrets.users ?? `${source.name}-secrets` }
  return [
    create(object),
    create({
      apiVersion: "psmdb.percona.com/v1",
      kind: "PerconaServerMongoDBRestore",
      metadata: { name: `${name}-restore` },
      spec: { clusterName: name, backupSource: { type: "logical", destination: point.from, s3 } },
    }),
  ]
}

function restoreMysql({ source, point, project, spec, store, object }: Restoring): Change[] {
  delete spec.backupPolicyName
  delete spec.offline
  const { bucket } = splitDestination(store?.destination)
  const endpoint = store?.endpoint ? { endpointURL: store.endpoint } : {}
  const storageClass = source.storageClass ? { storageClassName: source.storageClass } : {}
  spec.restore = {
    sourceName: source.name,
    sourceNamespace: project,
    restorePoint: point.from,
    jobConfig: {
      serviceAccountName: "default",
      bucketConfig: { bucketName: bucket, ...endpoint, usePathStyle: true },
      envFrom: [{ secretRef: { name: store?.secret ?? `${source.name}-backups-s3` } }],
      workVolume: {
        ephemeral: {
          volumeClaimTemplate: {
            spec: {
              accessModes: ["ReadWriteOnce"],
              ...storageClass,
              resources: { requests: { storage: source.storage ?? "10Gi" } },
            },
          },
        },
      },
    },
  }
  return [create(object)]
}

// The copy keeps the original's root password, which the dump holds.
function restoreMariadb({ point, name, spec, store, object, secret }: Restoring): Change[] {
  delete spec.suspend
  spec.bootstrapFrom = {
    backupRef: { name: store?.name },
    ...(point.from ? { targetRecoveryTime: point.from } : {}),
  }
  if (!spec.passwordSecretKeyRef) return [create(object)]
  spec.passwordSecretKeyRef = { name: `${name}-app`, key: "password", generate: false }
  return [secret(`${name}-app`), create(object)]
}

function restoreDragonfly({ name, spec, store, object, secret }: Restoring): Change[] {
  const { bucket } = splitDestination(store?.destination)
  const to = `s3://${bucket}/${name}`
  const aws = ((spec.env as Manifest[] | undefined) ?? []).filter((e) =>
    String(e.name).startsWith("AWS_"),
  )
  const others = ((spec.initContainers as Manifest[] | undefined) ?? []).filter(
    (c) => c.name !== "zeabur-restore",
  )
  spec.snapshot = { ...(spec.snapshot as Manifest | undefined), dir: to }
  spec.initContainers = [
    ...others,
    {
      name: "zeabur-restore",
      image: AWS_CLI,
      command: ["sh", "-c", COPY_SNAPSHOT],
      env: [
        { name: "FROM", value: store?.destination ?? "" },
        { name: "TO", value: to },
        { name: "ENDPOINT", value: store?.endpoint ?? "" },
        { name: "HOME", value: "/tmp" },
        ...aws,
      ],
      securityContext: { allowPrivilegeEscalation: false, capabilities: { drop: ["ALL"] } },
    },
  ]
  if (!spec.authentication) return [create(object)]
  spec.authentication = { passwordFromSecret: { name: `${name}-auth`, key: "password" } }
  return [secret(`${name}-auth`), create(object)]
}

const RESTORE = {
  postgresql: restorePostgresql,
  mongodb: restoreMongodb,
  mysql: restoreMysql,
  mariadb: restoreMariadb,
  dragonfly: restoreDragonfly,
}

/** The copy's metadata: its name, the original's finalizers, and where it came from. MOCO's
 * app user is in the dump, so the copy reads the original's Secret. */
function copyMetadata(
  original: Manifest | undefined,
  source: string,
  point: RestorePoint,
  name: string,
) {
  const annotations = (original?.annotations as Record<string, string> | undefined) ?? {}
  const kept = Object.entries(annotations).filter(
    ([key]) => key === APP_DATABASE || key === APP_SECRET,
  )
  return {
    name,
    ...(original?.finalizers ? { finalizers: original.finalizers } : {}),
    annotations: {
      ...Object.fromEntries(kept),
      "zeabur.com/restored-from": source,
      "zeabur.com/restored-backup": point.backup,
      ...(point.time ? { "zeabur.com/restored-time": point.time } : {}),
    },
  }
}

/** The writes that make `name` from `point` of `source`: a copy of its object that starts from
 * the backup, Percona's restore, and a password of its own where Zeabur keeps one. */
export function restoreChanges(
  source: Database,
  point: RestorePoint,
  name: string,
  project: string,
  password = newPassword,
): Change[] {
  const original = source.object as Manifest & { metadata?: Manifest; spec?: Manifest }
  const spec = structuredClone(original.spec ?? {})
  const object = {
    apiVersion: source.apiVersion,
    kind: source.kind,
    metadata: copyMetadata(original.metadata, source.name, point, name),
    spec,
  }
  const secret = (secretName: string) =>
    create({
      apiVersion: "v1",
      kind: "Secret",
      metadata: { name: secretName },
      type: "Opaque",
      stringData: { password: password() },
    })
  return RESTORE[source.engine]({
    source,
    point,
    name,
    project,
    spec,
    store: source.backups.store ?? undefined,
    object,
    secret,
  })
}

/** The database `database` was restored from, and the ones restored from it. */
export function restoreKin(database: Database, all: Database[]) {
  const same = all.filter((d) => d.engine === database.engine && d.name !== database.name)
  return {
    source: database.restored ? same.find((d) => d.name === database.restored?.from) : undefined,
    copies: same.filter((d) => d.restored?.from === database.name),
  }
}

type Repoint = Pick<Database, "engine" | "name" | "connection" | "app">

type Env = Target["containers"][number]["env"][number]

/** The Secret of `to` that matches the one of `from` that `secret` names. */
function matchingSecret(secret: string, from: Repoint, to: Repoint) {
  if (secret === from.app?.secret) return to.app?.secret
  if (secret === from.connection.secret) return to.connection.secret
  return undefined
}

/** The edit that makes one variable read `to` instead of `from`, if it needs one. */
function repoint(env: Env, from: Repoint, to: Repoint): EnvEdit | undefined {
  const at = `@${from.connection.host}:`
  if (env.source.kind === "Secret") {
    const name = env.source.name ? matchingSecret(env.source.name, from, to) : undefined
    if (!name || name === env.source.name) return undefined
    return { op: "set", name: env.name, valueFrom: { secretName: name, key: env.source.key } }
  }
  if (env.source.kind !== "Literal" || !env.value?.includes(at)) return undefined
  return { op: "set", name: env.name, value: env.value.replaceAll(at, `@${to.connection.host}:`) }
}

/** The variables that read `from`, set to read `to` instead: Secrets by the matching Secret of
 * `to`, URLs by its host. One change per container that has any; none for the rest. */
export function pointServices(from: Database, to: Repoint, targets: Target[]): Change[] {
  const host = `@${from.connection.host}:`
  const reads = (env: Env, all: Env[]) =>
    databaseOfEnv(env, all, [from]) === from || env.value?.includes(host)
  return targets.flatMap((target) =>
    target.containers.flatMap((container): Change[] => {
      const edits = container.env
        .filter((env) => reads(env, container.env))
        .map((env) => repoint(env, from, to))
        .filter((edit) => edit !== undefined)
      if (edits.length === 0) return []
      return [
        {
          op: "env",
          workload: { kind: target.kind, name: target.name },
          ...(target.containers.length > 1 ? { container: container.name } : {}),
          changes: edits,
        },
      ]
    }),
  )
}
