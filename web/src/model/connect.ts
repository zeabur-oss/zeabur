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

export interface Naming {
  /** The main variable; the suggested one when absent. */
  name?: string
  extras?: boolean
}

/** What goes before the companions' standard names: `ORDERS_` for `ORDERS_DATABASE_URL`,
 * nothing for `DATABASE_URL`. */
function stemOf(database: Pick<Database, "engine">, name: string) {
  const stem = name.replace(new RegExp(`_?(${mainVariable(database)}|URL|URI)$`), "")
  return stem ? `${stem}_` : ""
}

/** Every variable a connection under `name` sets: the main one, MongoDB's password, PG*. */
export function namesOf(database: Pick<Database, "engine">, name: string, extras = false) {
  const stem = stemOf(database, name)
  return {
    main: name,
    password: database.engine === "mongodb" ? `${stem}MONGOPASSWORD` : undefined,
    extras: database.engine === "postgresql" && extras ? pgExtras.map((e) => `${stem}${e}`) : [],
  }
}

const allNames = (names: ReturnType<typeof namesOf>) => [
  ...(names.password ? [names.password] : []),
  names.main,
  ...names.extras,
]

/** The first of the database's variable names, then the same under its prefix, that none of
 * the services has yet. */
export function suggestedName(
  database: Pick<Database, "engine" | "name">,
  targets: Target[],
  extras = false,
): string {
  const main = mainVariable(database)
  const prefix = prefixOf(database)
  const tries = [main, `${prefix}${main}`]
  for (let n = 2; n < 100; n++) tries.push(`${prefix}${n}_${main}`)
  return (
    tries.find(
      (name) => takenIn(targets, allNames(namesOf(database, name, extras))) === undefined,
    ) ?? main
  )
}

/** The first of `names` a service already has, and that service. */
function takenIn(targets: Target[], names: string[]) {
  for (const target of targets) {
    const have = new Set(envOf(target).map((e) => e.name))
    const name = names.find((n) => have.has(n))
    if (name) return { name, service: target.name }
  }
  return undefined
}

export type NameProblem = { kind: "invalid" } | { kind: "taken"; name: string; service: string }

/** Why the connection cannot use `name` in these services. */
export function nameProblem(
  database: Pick<Database, "engine">,
  targets: Target[],
  name: string,
  extras = false,
): NameProblem | undefined {
  if (variableProblem(name)) return { kind: "invalid" }
  const taken = takenIn(targets, allNames(namesOf(database, name, extras)))
  return taken ? { kind: "taken", ...taken } : undefined
}

/** The variables one service gets: MongoDB's password before the URL that reads it. */
export function variablesFor(
  database: Connectable,
  target: Target,
  { name, extras = false }: Naming = {},
): EnvEdit[] {
  const app = database.app
  if (!app) return []
  const names = namesOf(database, name ?? suggestedName(database, [target], extras), extras)
  const secret = (variable: string, key: string): EnvEdit => ({
    op: "set",
    name: variable,
    valueFrom: { secretName: app.secret, key },
  })
  if (database.engine === "postgresql")
    return [
      secret(names.main, "uri"),
      ...(extras ? PG_EXTRAS.map(([, key], i) => secret(names.extras[i] ?? "", key)) : []),
    ]
  const password = names.password ?? "MONGOPASSWORD"
  const { host, port, options } = database.connection
  const query = [options, `authSource=${app.database}`].filter(Boolean).join("&")
  return [
    secret(password, app.passwordKey),
    {
      op: "set",
      name: names.main,
      value: `mongodb://${app.username}:$(${password})@${host}:${port}/${app.database}?${query}`,
    },
  ]
}

/** The plan's changes: the variables, for each service. */
export function connectChanges(
  database: Connectable,
  targets: Target[],
  naming: Naming = {},
): Change[] {
  const name = naming.name ?? suggestedName(database, targets, naming.extras)
  return targets.map((target) => ({
    op: "env",
    workload: { kind: target.kind, name: target.name },
    ...(target.containers.length > 1 ? { container: target.containers[0]?.name } : {}),
    changes: variablesFor(database, target, { ...naming, name }),
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

const references = (env: Env) =>
  env.source.kind === "Literal"
    ? [...(env.value ?? "").matchAll(/\$\(([A-Za-z_][A-Za-z0-9_]*)\)/g)].map((m) => m[1] ?? "")
    : []

/** The database a Secret-backed variable reads: its app user's Secret, or the one its connection
 * names. */
const bySecret = (env: Env, databases: Database[]) =>
  env.source.kind === "Secret"
    ? databases.find(
        (d) => d.app?.secret === env.source.name || d.connection.secret === env.source.name,
      )
    : undefined

/** The database a variable of a container reads: from its Secret, or, for `MONGO_URL`, through
 * a `$(VAR)` that does. */
export function databaseOfEnv(
  env: Env,
  siblings: Env[],
  databases: Database[],
): Database | undefined {
  const direct = bySecret(env, databases)
  if (direct || env.source.kind !== "Literal") return direct
  for (const name of references(env)) {
    const sibling = siblings.find((e) => e.name === name)
    const found = sibling ? bySecret(sibling, databases) : undefined
    if (found) return found
  }
  return undefined
}

/** What disconnecting `variable` removes from a container: it, and for MongoDB the URL and
 * password that need each other. A password another URL still reads stays. */
export function disconnection(
  variable: string,
  env: Env[],
  databases: Database[],
): { database: Database; names: string[] } | undefined {
  const own = env.find((e) => e.name === variable)
  const database = own ? databaseOfEnv(own, env, databases) : undefined
  if (!own || !database) return undefined
  const going = new Set([variable])
  const urls = own.source.kind === "Literal" ? [own] : []
  if (own.source.kind === "Secret")
    for (const e of env)
      if (references(e).includes(variable) && databaseOfEnv(e, env, databases) === database) {
        going.add(e.name)
        urls.push(e)
      }
  for (const url of urls)
    for (const name of references(url)) {
      const sibling = env.find((e) => e.name === name)
      const elsewhere = env.some((e) => !going.has(e.name) && references(e).includes(name))
      if (sibling && bySecret(sibling, databases) === database && !elsewhere) going.add(name)
    }
  return { database, names: env.map((e) => e.name).filter((n) => going.has(n)) }
}

/** The plan's change for disconnecting: the variables removed, the database left as it is. */
export function disconnectChange(target: Target, container: string, names: string[]): Change {
  return {
    op: "env",
    workload: { kind: target.kind, name: target.name },
    ...(target.containers.length > 1 ? { container } : {}),
    changes: names.map((name) => ({ op: "remove", name })),
  }
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
