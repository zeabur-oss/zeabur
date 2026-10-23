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

const ENGINE: Record<Counterpart, Engine | undefined> = {
  postgresql: "postgresql",
  mysql: "mysql",
  mariadb: "mariadb",
  mongodb: "mongodb",
  dragonfly: "dragonfly",
  objectStorage: undefined,
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
export type KeptReason = "noOperator" | "notInstalled" | "unknown" | "role"

/** For each service with a counterpart: the engine Zeabur can create, or why not. */
export function managedOffer(
  service: ComposeService,
  operators: Partial<Record<Engine, Schemas["DatabaseOperator"]["state"]>>,
  mayCreate: boolean,
): { engine: Engine } | { kept: KeptReason } | undefined {
  if (!service.counterpart || !service.image) return undefined
  const engine = ENGINE[service.counterpart]
  if (!engine) return { kept: "noOperator" }
  if (!mayCreate) return { kept: "role" }
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

const withPort = (host: string, value: string) => {
  const port = value.split(":")[1]
  return port ? `${host}:${port}` : host
}

/** A variable rewritten for a database an operator makes, by what it holds. */
const REWRITE: Record<
  Schemas["Part"],
  (v: Variable, database: Connectable, app: App) => ServiceVariable[]
> = {
  url: urlVariables,
  host: (v, database) => [{ name: v.name, value: withPort(database.connection.host, v.value) }],
  password: (v, _, app) => [{ name: v.name, from: { secret: app.secret, key: app.passwordKey } }],
  user: (v, _, app) => [{ name: v.name, value: app.username ?? v.value }],
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

/** Every create of the import, databases first, and the project's level once. */
export function composeChanges(
  conversion: Conversion,
  choices: ComposeChoices,
  context: ComposeContext,
): Change[] {
  const databases = new Map<string, { form: NewDatabase; connection: Connectable }>()
  for (const service of conversion.services) {
    const engine = service.counterpart ? ENGINE[service.counterpart] : undefined
    if (!engine || !choices.managed.has(service.name)) continue
    const form = managedForm(service, engine, context.nodes)
    databases.set(service.name, { form, connection: pendingDatabase(form, context.project) })
  }
  const byName = new Map(conversion.services.map((s) => [s.name, s]))
  const reach = (name: string) => {
    const managed = databases.get(name)?.connection.connection
    if (managed) return { host: managed.host, port: managed.port }
    const target = byName.get(name)
    const port = target ? portsOf(target)[0] : undefined
    return port ? { host: name, port } : undefined
  }
  const changes: Change[] = [...databases.values()].flatMap(({ form }) => createDatabase(form))
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
