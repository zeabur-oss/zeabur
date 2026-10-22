import type { Schemas } from "@/api/client"
import type { Database, NewDatabase } from "@/model/databases"
import type { Service } from "@/model/service"

type Change = Schemas["Change"]
type EnvEdit = Schemas["EnvEdit"]
type Env = Schemas["EnvVar"]

export type Target = Pick<Service, "kind" | "name" | "containers">
/** A database as far as connecting to it goes; also one not created yet. */
export type Connectable = Pick<Database, "engine" | "name" | "connection" | "app">

/** The variable a service reads the database from, as Railway names it. */
export const mainVariable = (database: Pick<Database, "engine">) =>
  database.engine === "postgresql" ? "DATABASE_URL" : "MONGO_URL"

const PG_EXTRAS = [
  ["PGHOST", "host"],
  ["PGPORT", "port"],
  ["PGUSER", "username"],
  ["PGPASSWORD", "password"],
  ["PGDATABASE", "dbname"],
] as const

export const pgExtras = PG_EXTRAS.map(([name]) => name)

/** `<NAME>_` from the database's name. */
const prefixOf = (database: Pick<Database, "name">) =>
  `${database.name.toUpperCase().replace(/[^A-Z0-9]/g, "_")}_`

const envOf = (target: Target): Env[] => target.containers[0]?.env ?? []

/** The Secret holding the app user's password, which every connection reads. */
const secretOf = (database: Connectable) => database.app?.secret

/** Whether a service already reads this database. */
export const connected = (target: Target, database: Connectable) =>
  target.containers.some((c) =>
    c.env.some((e) => e.source.kind === "Secret" && e.source.name === secretOf(database)),
  )

/** The variables one service gets; a name the service already has gets the database's prefix. */
export function variablesFor(
  database: Connectable,
  target: Target,
  { name = mainVariable(database), extras = false }: { name?: string; extras?: boolean } = {},
): EnvEdit[] {
  const app = database.app
  if (!app) return []
  const taken = new Set(envOf(target).map((e) => e.name))
  const prefix = taken.has(name) ? prefixOf(database) : ""
  const secret = (variable: string, key: string): EnvEdit => ({
    op: "set",
    name: `${prefix}${variable}`,
    valueFrom: { secretName: app.secret, key },
  })
  if (database.engine === "postgresql")
    return [
      secret(name, "uri"),
      ...(extras ? PG_EXTRAS.map(([variable, key]) => secret(variable, key)) : []),
    ]
  const password = secret("MONGOPASSWORD", app.passwordKey)
  const { host, port, options } = database.connection
  const query = [options, `authSource=${app.database}`].filter(Boolean).join("&")
  return [
    password,
    {
      op: "set",
      name: `${prefix}${name}`,
      value: `mongodb://${app.username}:$(${password.name})@${host}:${port}/${app.database}?${query}`,
    },
  ]
}

/** The plan's changes: the variables, for each service. */
export function connectChanges(
  database: Connectable,
  targets: Target[],
  options: { name?: string; extras?: boolean } = {},
): Change[] {
  return targets.map((target) => ({
    op: "env",
    workload: { kind: target.kind, name: target.name },
    ...(target.containers.length > 1 ? { container: target.containers[0]?.name } : {}),
    changes: variablesFor(database, target, options),
  }))
}

/** Workloads whose variables can be set; the database's own and other tools' are left out. */
export function candidates(services: Service[], rights: Schemas["WorkloadPatch"][]): Service[] {
  return services.filter((s) => {
    if (s.dataStore || s.provenance.tool) return false
    const right = rights.find((r) => r.kind === s.kind)
    return !!right && (right.all !== "denied" || right.names.includes(s.name))
  })
}

/** The database a variable of a container reads: from its Secret, or, for `MONGO_URL`, through
 * a `$(VAR)` that does. */
export function databaseOfEnv(
  env: Env,
  siblings: Env[],
  databases: Database[],
): Database | undefined {
  const bySecret = (e: Env) =>
    e.source.kind === "Secret"
      ? databases.find((d) => d.app && d.app.secret === e.source.name)
      : undefined
  const direct = bySecret(env)
  if (direct || env.source.kind !== "Literal") return direct
  const value = env.value ?? ""
  for (const match of value.matchAll(/\$\(([A-Za-z_][A-Za-z0-9_]*)\)/g)) {
    const sibling = siblings.find((e) => e.name === match[1])
    const found = sibling ? bySecret(sibling) : undefined
    if (found) return found
  }
  return undefined
}

/** What a database the drawer is about to create will be, for connecting services to it. */
export function pendingDatabase(form: NewDatabase, project: string): Connectable {
  const app = { username: "app", database: "app" }
  if (form.engine === "postgresql")
    return {
      engine: form.engine,
      name: form.name,
      connection: {
        host: `${form.name}-rw.${project}.svc`,
        port: 5432,
        secret: `${form.name}-app`,
        passwordKey: "password",
      },
      app: { ...app, secret: `${form.name}-app`, passwordKey: "password" },
    }
  return {
    engine: form.engine,
    name: form.name,
    connection: {
      host: `${form.name}-rs0.${project}.svc`,
      port: 27017,
      secret: `${form.name}-secrets`,
      passwordKey: "MONGODB_DATABASE_ADMIN_PASSWORD",
      options: "replicaSet=rs0",
    },
    app: { ...app, secret: `${form.name}-custom-user-secret`, passwordKey: "app" },
  }
}

export const variableProblem = (name: string) => !/^[A-Za-z_][A-Za-z0-9_]*$/.test(name)
