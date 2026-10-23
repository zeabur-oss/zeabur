// A database's operator runs workloads and volumes of its own: CloudNativePG's instance pods and
// their claims, Percona's `<name>-rs0` StatefulSet and `mongod-data-…` claims. The server marks a
// workload a database's operator controls with `managedBy` (its controller owner reference); the
// project's pages show those as the database, not as services.
import type { Edge } from "@/model/connections"
import { connected } from "@/model/connect"
import { engineName, stateHealth, type Database } from "@/model/databases"
import { serviceId, type Claim, type Health, type Project, type Service } from "@/model/service"
import { bytesOf } from "@/model/volume-edit"

/** Whether a database's operator runs this workload for the database. */
const isDatabasePart = (service: Pick<Service, "managedBy">) => Boolean(service.managedBy?.engine)

const key = (ref: { kind: string; name: string }) => `${ref.kind}/${ref.name}`

/** The project without its databases' own workloads, nor the connections to them. */
export function foldDatabases(project: Project): Project {
  const parts = new Set(project.services.filter(isDatabasePart).map(key))
  if (parts.size === 0) return project
  return {
    ...project,
    services: project.services.filter((s) => !parts.has(key(s))),
    connections: project.connections.filter(
      (c) => !parts.has(key(c.from)) && !parts.has(key(c.to)),
    ),
  }
}

/** Engine words as the server sends them in `managedBy` (`MongoDB`), by the API's engine id. */
const ENGINE_OF: Record<string, Database["engine"]> = {
  PostgreSQL: "postgresql",
  MongoDB: "mongodb",
  MySQL: "mysql",
  MariaDB: "mariadb",
  Dragonfly: "dragonfly",
}

/** The database a part belongs to, among `databases`. */
const databaseOfPart = (service: Service, databases: Database[]) =>
  databases.find(
    (d) =>
      d.name === service.managedBy?.name && d.engine === ENGINE_OF[service.managedBy.engine ?? ""],
  )

/** The workloads the database's operator runs for it. */
export const partsOf = (database: Database, services: Service[]) =>
  services.filter(
    (s) =>
      s.managedBy?.name === database.name &&
      ENGINE_OF[s.managedBy.engine ?? ""] === database.engine,
  )

/** A database on the project map. */
export interface DatabaseNode {
  /** `database:<engine>/<name>`. */
  id: string
  database: Database
  health: Health
  /** Its members' volumes. */
  volumes: { name: string; size?: string | null }[]
}

const databaseNodeId = (database: Pick<Database, "engine" | "name">) =>
  `database:${database.engine}/${database.name}`

/** `claims` from the whole project give a volume its size when the database has none. */
export function databaseNodes(databases: Database[], claims: Claim[] = []): DatabaseNode[] {
  return databases.map((database) => ({
    id: databaseNodeId(database),
    database,
    health: stateHealth[database.state],
    volumes: database.claims.map((name) => ({
      name,
      size: database.storage ?? claims.find((c) => c.name === name)?.size,
    })),
  }))
}

/**
 * Edges to the databases: the connections the server found to a database's workloads, and each
 * service that reads the database's app user Secret. `project` is the whole project, before
 * folding.
 */
export function databaseEdges(project: Project, databases: Database[]): Edge[] {
  const edges = new Map<string, Edge>()
  for (const connection of project.connections) {
    const part = project.services.find((s) => key(s) === key(connection.to))
    const database = part && isDatabasePart(part) ? databaseOfPart(part, databases) : undefined
    const from = project.services.find((s) => key(s) === key(connection.from))
    if (!database || !from || isDatabasePart(from)) continue
    const id = `${serviceId(from)}->${databaseNodeId(database)}`
    edges.set(id, {
      id,
      from: serviceId(from),
      to: databaseNodeId(database),
      inferred: connection.inferred,
      evidence: connection.evidence,
    })
  }
  for (const database of databases) {
    const secret = database.app?.secret
    if (!secret) continue
    for (const service of project.services) {
      if (isDatabasePart(service) || !connected(service, database)) continue
      const id = `${serviceId(service)}->${databaseNodeId(database)}`
      if (edges.has(id)) continue
      edges.set(id, {
        id,
        from: serviceId(service),
        to: databaseNodeId(database),
        inferred: false,
        evidence: [{ basis: "sharedObject", object: { kind: "Secret", name: secret } }],
      })
    }
  }
  return [...edges.values()]
}

/** The version as the create form offers it: PostgreSQL's major, the others' major.minor. */
export function versionLabel(engine: Database["engine"], version?: string | null): string {
  if (!version) return ""
  const match = engine === "postgresql" ? /^(\d+)/.exec(version) : /^(\d+\.\d+)/.exec(version)
  return match?.[1] ?? version
}

/** `MongoDB 7.0`. */
export const engineVersion = (database: Pick<Database, "engine" | "version">) =>
  [engineName(database.engine), versionLabel(database.engine, database.version)]
    .filter(Boolean)
    .join(" ")

/** One row of the Storage card: a volume, or a database's volumes together. */
export interface StorageRow {
  key: string
  claims: Claim[]
  database?: Database
  /** Bytes asked for in all, when known. */
  bytes?: number
}

/**
 * Volumes as Storage lists them. `claims` come from the whole project, so a database's volumes
 * still carry their size; a volume a database's operator made is one row with the database's
 * others, and its services are the ones outside the database.
 */
export function storageRows(claims: Claim[], databases: Database[]): StorageRow[] {
  const rows: StorageRow[] = []
  const byDatabase = new Map<string, StorageRow>()
  for (const claim of claims) {
    const part = claim.services.find(isDatabasePart)
    const database =
      databases.find((d) => d.claims.includes(claim.name)) ??
      (part ? databaseOfPart(part, databases) : undefined)
    const services = claim.services.filter((s) => !isDatabasePart(s))
    const own = { ...claim, services, orphan: claim.orphan && !database }
    const bytes = claim.sizeBytes ?? (database?.storage ? bytesOf(database.storage) : undefined)
    if (!database || services.length > 0) {
      rows.push({ key: claim.name, claims: [own], bytes: claim.sizeBytes ?? undefined })
      continue
    }
    const id = databaseNodeId(database)
    const row = byDatabase.get(id)
    if (row) {
      row.claims.push(own)
      row.bytes = row.bytes === undefined || bytes === undefined ? undefined : row.bytes + bytes
    } else {
      const created: StorageRow = { key: id, claims: [own], database, bytes }
      byDatabase.set(id, created)
      rows.push(created)
    }
  }
  return rows
}
