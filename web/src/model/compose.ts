// A Docker Compose file, as the server reads it (`POST /projects/{p}/compose`), turned into the
// objects the "Docker image" and database forms create, in one plan.
import type { Schemas } from "@/api/client"
import { pendingDatabase, variablesFor, type Connectable } from "@/model/connect"
import {
  createDatabase,
  newDatabase,
  versions,
  type Engine,
  type NewDatabase,
} from "@/model/databases"
import {
  bucketProblem,
  createObjectStorage,
  newObjectStorage,
  pendingObjectStorage,
  type NewObjectStorage,
} from "@/model/object-storage"
import {
  imageObjects,
  newImageService,
  projectLevel,
  suggestedHost,
  type DomainSettings,
  type ImageService,
  type ServiceVariable,
} from "@/model/image-service"

type Change = Schemas["Change"]
export type Conversion = Schemas["Conversion"]
export type ComposeService = Schemas["ComposeService"]
type Counterpart = Schemas["Counterpart"]

/** What an operator makes for a Compose service: a database engine, or object storage. */
export type Managed = Engine | "objectStorage"

const ENGINE: Record<Counterpart, Managed> = {
  postgresql: "postgresql",
  mysql: "mysql",
  mariadb: "mariadb",
  mongodb: "mongodb",
  dragonfly: "dragonfly",
  objectStorage: "objectStorage",
}

/** The port a database image listens on when the Compose file names none. */
const DEFAULT_PORT: Record<Counterpart, number> = {
  postgresql: 5432,
  mysql: 3306,
  mariadb: 3306,
  mongodb: 27017,
  dragonfly: 6379,
  objectStorage: 9000,
}

const VOLUME_SIZE = "1Gi"

/** Why a database image stays a plain image. */
export type KeptReason = "notInstalled" | "unknown" | "role"

/** For each service with a counterpart: what Zeabur can create, or why not. */
export function managedOffer(
  service: ComposeService,
  operators: Partial<Record<Managed, Schemas["ProviderState"]>>,
  mayCreate: (engine: Managed) => boolean,
): { engine: Managed } | { kept: KeptReason } | undefined {
  if (!service.counterpart || !service.image) return undefined
  const engine = ENGINE[service.counterpart]
  if (!mayCreate(engine)) return { kept: "role" }
  const state = operators[engine]
  if (state === "installed") return { engine }
  return { kept: state === "not-installed" ? "notInstalled" : "unknown" }
}

export interface ComposeContext {
  project: string
  domains: DomainSettings
  enforced?: Schemas["PodSecurityLevel"] | null
  /** Which parts of a service the person's rights allow (`imageOptions`). */
  options: { port: boolean; publicUrl: boolean; secrets: boolean; volume: boolean }
  nodes: number
}

export interface ComposeChoices {
  /** Services made by an operator, by name; the others run their image. */
  managed: ReadonlySet<string>
  /** Services with a public domain, and the domain. */
  published: ReadonlyMap<string, string>
}

/** The version label for the Compose image's tag, when the form offers it. */
function versionOf(engine: Engine, image: string): string | undefined {
  const tag = image.split("@")[0]?.split("/").at(-1)?.split(":")[1] ?? ""
  return versions[engine].find((v) => tag === v.label || tag.startsWith(`${v.label}.`))?.label
}

function managedForm(service: ComposeService, engine: Engine, nodes: number): NewDatabase {
  const form = newDatabase(engine, nodes)
  const version = versionOf(engine, service.image ?? "")
  return { ...form, name: service.name, ...(version ? { version } : {}) }
}

type Variable = Schemas["ComposeVariable"]
type App = NonNullable<Connectable["app"]>
const kept = (v: Variable): ServiceVariable[] => [{ name: v.name, value: v.value }]

/** The connection variables the database forms give, under the variable's name. */
const urlVariables = (v: Variable, database: Connectable): ServiceVariable[] =>
  variablesFor(database, { kind: "Deployment", name: "", containers: [] }, { name: v.name }).map(
    (edit) =>
      edit.valueFrom
        ? {
            name: edit.name,
            from: { secret: edit.valueFrom.secretName ?? "", key: edit.valueFrom.key ?? "" },
          }
        : { name: edit.name, value: edit.value ?? "", expands: true },
  )

/** The host, with the port when the value has one: the value's own, or object storage's. */
const withPort = (database: Connectable, value: string) => {
  const { host, port } = database.connection
  const own = value.split(":")[1]
  if (!own) return host
  return `${host}:${database.engine === "s3" ? port : own}`
}

const s3Url = (v: Variable, database: Connectable): ServiceVariable[] => [
  { name: v.name, value: `http://${database.connection.host}:${database.connection.port}` },
]

/** A variable rewritten for a database an operator makes, by what it holds. */
const REWRITE: Record<
  Schemas["Part"],
  (v: Variable, database: Connectable, app: App) => ServiceVariable[]
> = {
  url: (v, database) => (database.engine === "s3" ? s3Url : urlVariables)(v, database),
  host: (v, database) => [{ name: v.name, value: withPort(database, v.value) }],
  password: (v, _, app) => [{ name: v.name, from: { secret: app.secret, key: app.passwordKey } }],
  user: (v, database, app) =>
    database.engine === "s3"
      ? [
          {
            name: v.name,
            from: { secret: app.secret, key: database.connection.usernameKey ?? "accessKey" },
          },
        ]
      : [{ name: v.name, value: app.username ?? v.value }],
  database: (v, _, app) => [{ name: v.name, value: app.database ?? v.value }],
  mention: kept,
}

function rewrite(v: Variable, database: Connectable | undefined): ServiceVariable[] {
  const part = v.refers?.part
  return database?.app && part ? REWRITE[part](v, database, database.app) : kept(v)
}

/** The first of each name; a later one would come after a $(…) that reads it. */
function firstOfEach(variables: ServiceVariable[]): ServiceVariable[] {
  const seen = new Set<string>()
  return variables.filter((v) => !seen.has(v.name) && seen.add(v.name))
}

/** The services the plan makes as images: those with an image and no operator chosen. */
export const imageServices = (conversion: Conversion, choices: ComposeChoices) =>
  conversion.services.filter((s) => s.image && !choices.managed.has(s.name))

/** The buckets a MinIO service's users name: its `MINIO_DEFAULT_BUCKETS`, and the `*BUCKET*`
 * variables of the services that reach it. */
function bucketsOf(conversion: Conversion, store: ComposeService): string[] {
  const own = store.variables
    .filter((v) => v.name === "MINIO_DEFAULT_BUCKETS")
    .flatMap((v) => v.value.split(/[,;\s]+/).map((b) => b.split(":")[0] ?? ""))
  const users = conversion.services.filter((s) =>
    s.variables.some((v) => v.refers?.service === store.name),
  )
  const named = users.flatMap((s) =>
    s.variables.filter((v) => /BUCKET/.test(v.name) && !v.refers).map((v) => v.value.trim()),
  )
  return [...new Set([...own, ...named])].filter((b) => !bucketProblem(b, []))
}

function storeForm(conversion: Conversion, service: ComposeService): NewObjectStorage {
  return { ...newObjectStorage(), name: service.name, buckets: bucketsOf(conversion, service) }
}

/** How a service an operator makes differs from its Compose service. */
export type Difference =
  | { kind: "version"; image: string; version: string }
  | { kind: "command" }
  | { kind: "ports"; from: number[]; to: number }
  | { kind: "volumes"; paths: string[]; gib: number }
  | { kind: "variables"; names: string[] }
  | { kind: "instances"; from: number; to: number }

/** The port, storage, instances and version of what the operator makes for a service. */
function managedTarget(
  conversion: Conversion,
  service: ComposeService,
  engine: Managed,
  { project, nodes }: Pick<ComposeContext, "project" | "nodes">,
) {
  if (engine === "objectStorage") {
    const form = storeForm(conversion, service)
    const port = pendingObjectStorage(form, project).connection.port
    return { port, storage: form.storage, members: 1, version: undefined }
  }
  const form = managedForm(service, engine, nodes)
  const port = pendingDatabase(form, project).connection.port
  const version = versionOf(engine, service.image ?? "") ? undefined : form.version
  return { port, storage: form.storage, members: form.members, version }
}

/** What the operator's service does differently from the Compose service it replaces. */
export function differences(
  conversion: Conversion,
  service: ComposeService,
  engine: Managed,
  context: Pick<ComposeContext, "project" | "nodes">,
): Difference[] {
  const target = managedTarget(conversion, service, engine, context)
  const out: Difference[] = []
  if (target.version)
    out.push({ kind: "version", image: service.image ?? "", version: target.version })
  if (service.command?.length || service.args?.length) out.push({ kind: "command" })
  if (service.ports.some((p) => p !== target.port))
    out.push({ kind: "ports", from: service.ports, to: target.port })
  if (service.volumes.length > 0)
    out.push({
      kind: "volumes",
      paths: service.volumes.map((v) => v.mountPath),
      gib: Number.parseInt(target.storage, 10),
    })
  if (service.variables.length > 0)
    out.push({ kind: "variables", names: service.variables.map((v) => v.name) })
  if (target.members !== service.replicas)
    out.push({ kind: "instances", from: service.replicas, to: target.members })
  return out
}

/** Every create of the import, databases first, and the project's level once. */
export function composeChanges(
  conversion: Conversion,
  choices: ComposeChoices,
  context: ComposeContext,
): Change[] {
  const databases = new Map<string, { changes: Change[]; connection: Connectable }>()
  for (const service of conversion.services) {
    const engine = service.counterpart ? ENGINE[service.counterpart] : undefined
    if (!engine || !choices.managed.has(service.name)) continue
    if (engine === "objectStorage") {
      const form = storeForm(conversion, service)
      databases.set(service.name, {
        changes: createObjectStorage(form),
        connection: pendingObjectStorage(form, context.project),
      })
      continue
    }
    const form = managedForm(service, engine, context.nodes)
    databases.set(service.name, {
      changes: createDatabase(form),
      connection: pendingDatabase(form, context.project),
    })
  }
  const byName = new Map(conversion.services.map((s) => [s.name, s]))
  const reach = (name: string) => {
    const managed = databases.get(name)?.connection.connection
    if (managed) return { host: managed.host, port: managed.port }
    const target = byName.get(name)
    const port = target ? portsOf(target)[0] : undefined
    return port ? { host: name, port } : undefined
  }
  const changes: Change[] = [...databases.values()].flatMap((d) => d.changes)
  for (const service of imageServices(conversion, choices)) {
    const form = imageService(service, choices, context, (variable) =>
      rewrite(variable, databases.get(variable.refers?.service ?? "")?.connection),
    )
    const hosts = service.dependsOn.flatMap((d) => reach(d) ?? [])
    if (hosts.length > 0) form.waitFor = { hosts, image: conversion.waitImage }
    changes.push(
      ...imageObjects(form, context.domains).map((object): Change => ({
        op: "create",
        object: object as Record<string, never>,
      })),
    )
  }
  return [...changes, ...projectLevel("baseline", context.enforced)]
}

/** The service's ports; a database image without any listens on its usual one. */
export const portsOf = (service: ComposeService): number[] =>
  service.ports.length > 0
    ? service.ports
    : service.counterpart
      ? [DEFAULT_PORT[service.counterpart]]
      : []

function imageService(
  service: ComposeService,
  choices: ComposeChoices,
  { project, domains, options }: ComposeContext,
  variable: (v: Schemas["ComposeVariable"]) => ServiceVariable[],
): ImageService {
  const [port, ...morePorts] = options.port ? portsOf(service) : []
  const host = choices.published.get(service.name)
  const volumes = options.volume
    ? service.volumes.map((v) => ({
        name: v.name,
        mountPath: v.mountPath,
        size: VOLUME_SIZE,
        readOnly: v.readOnly,
      }))
    : []
  return {
    ...newImageService(),
    image: service.image ?? "",
    name: service.name,
    port: port ? String(port) : "",
    morePorts,
    publicUrl: Boolean(port && options.publicUrl && host !== undefined),
    host: host || suggestedHost(service.name, project, domains.suffix),
    variables: firstOfEach(service.variables.flatMap(variable)),
    secretVariables: options.secrets,
    volumes,
    replicas: volumes.length > 0 ? 1 : service.replicas,
    // Compose images often run as root; Compatible lets them start.
    security: "baseline",
    command: service.command,
    args: service.args,
  }
}
