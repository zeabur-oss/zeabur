// Restoring a backup into a new database, and moving services between the two. The new database
// copies the original's spec and starts from the backup; the original is not changed. The
// `zeabur.com/restored-*` annotations tie the two together for the pages.
import type { Schemas } from "@/api/client"
import { databaseOfEnv, type Target } from "@/model/connect"
import { newPassword, splitDestination, type Database } from "@/model/databases"

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

/** The writes that make `name` from `point` of `source`: a copy of its object that starts from
 * the backup, Percona's restore, and a password of its own where Zeabur keeps one. MariaDB keeps
 * the original's root password, which the dump holds. */
export function restoreChanges(
  source: Database,
  point: RestorePoint,
  name: string,
  project: string,
  password = newPassword,
): Change[] {
  const original = source.object as Manifest & { metadata?: Manifest; spec?: Manifest }
  const spec = structuredClone(original.spec ?? {})
  const finalizers = original.metadata?.finalizers
  const object = {
    apiVersion: source.apiVersion,
    kind: source.kind,
    metadata: {
      name,
      ...(finalizers ? { finalizers } : {}),
      annotations: {
        "zeabur.com/restored-from": source.name,
        "zeabur.com/restored-backup": point.backup,
        ...(point.time ? { "zeabur.com/restored-time": point.time } : {}),
      },
    },
    spec,
  }
  const store = source.backups.store
  const create = (body: Manifest): Change => ({
    op: "create",
    object: body as Record<string, never>,
  })
  const secret = (secretName: string) =>
    create({
      apiVersion: "v1",
      kind: "Secret",
      metadata: { name: secretName },
      type: "Opaque",
      stringData: { password: password() },
    })
  switch (source.engine) {
    case "postgresql":
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
    case "mongodb": {
      // A logical dump holds the system users, so the copy keeps the original's users Secret.
      const backup = (spec.backup as Manifest | undefined) ?? {}
      const storages = (backup.storages as Record<string, Manifest> | undefined) ?? {}
      const s3 = storages[store?.name ?? "backups"]?.s3
      spec.backup = { ...backup, tasks: [] }
      spec.secrets = {
        ...(spec.secrets as Manifest | undefined),
        users: (original.spec?.secrets as Manifest | undefined)?.users ?? `${source.name}-secrets`,
      }
      return [
        create(object),
        create({
          apiVersion: "psmdb.percona.com/v1",
          kind: "PerconaServerMongoDBRestore",
          metadata: { name: `${name}-restore` },
          spec: {
            clusterName: name,
            backupSource: { type: "logical", destination: point.from, s3 },
          },
        }),
      ]
    }
    case "mysql": {
      delete spec.backupPolicyName
      delete spec.offline
      const { bucket } = splitDestination(store?.destination)
      spec.restore = {
        sourceName: source.name,
        sourceNamespace: project,
        restorePoint: point.from,
        jobConfig: {
          serviceAccountName: "default",
          bucketConfig: {
            bucketName: bucket,
            ...(store?.endpoint ? { endpointURL: store.endpoint } : {}),
            usePathStyle: true,
          },
          envFrom: [{ secretRef: { name: store?.secret ?? `${source.name}-backups-s3` } }],
          workVolume: {
            ephemeral: {
              volumeClaimTemplate: {
                spec: {
                  accessModes: ["ReadWriteOnce"],
                  ...(source.storageClass ? { storageClassName: source.storageClass } : {}),
                  resources: { requests: { storage: source.storage ?? "10Gi" } },
                },
              },
            },
          },
        },
      }
      return [create(object)]
    }
    case "mariadb":
      delete spec.bootstrapFrom
      delete spec.suspend
      spec.bootstrapFrom = {
        backupRef: { name: store?.name },
        ...(point.from ? { targetRecoveryTime: point.from } : {}),
      }
      if (!spec.passwordSecretKeyRef) return [create(object)]
      spec.passwordSecretKeyRef = { name: `${name}-app`, key: "password", generate: false }
      return [secret(`${name}-app`), create(object)]
    case "dragonfly": {
      const { bucket } = splitDestination(store?.destination)
      const to = `s3://${bucket}/${name}`
      const snapshot = (spec.snapshot as Manifest | undefined) ?? {}
      spec.snapshot = { ...snapshot, dir: to }
      const aws = ((spec.env as Manifest[] | undefined) ?? []).filter((e) =>
        String(e.name).startsWith("AWS_"),
      )
      spec.initContainers = [
        ...((spec.initContainers as Manifest[] | undefined) ?? []).filter(
          (c) => c.name !== "zeabur-restore",
        ),
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
  }
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

/** The variables that read `from`, set to read `to` instead: Secrets by the matching Secret of
 * `to`, URLs by its host. One change per container that has any; none for the rest. */
export function pointServices(from: Database, to: Repoint, targets: Target[]): Change[] {
  const changes: Change[] = []
  const host = from.connection.host
  for (const target of targets)
    for (const container of target.containers) {
      const edits: EnvEdit[] = []
      for (const env of container.env) {
        if (databaseOfEnv(env, container.env, [from]) !== from && !env.value?.includes(`@${host}:`))
          continue
        if (env.source.kind === "Secret") {
          const name =
            env.source.name === from.app?.secret
              ? to.app?.secret
              : env.source.name === from.connection.secret
                ? to.connection.secret
                : undefined
          if (name && name !== env.source.name)
            edits.push({
              op: "set",
              name: env.name,
              valueFrom: { secretName: name, key: env.source.key },
            })
        } else if (env.source.kind === "Literal" && env.value?.includes(`@${host}:`)) {
          edits.push({
            op: "set",
            name: env.name,
            value: env.value.replaceAll(`@${host}:`, `@${to.connection.host}:`),
          })
        }
      }
      if (edits.length > 0)
        changes.push({
          op: "env",
          workload: { kind: target.kind, name: target.name },
          ...(target.containers.length > 1 ? { container: container.name } : {}),
          changes: edits,
        })
    }
  return changes
}
