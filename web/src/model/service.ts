// What the interface derives from one project's API answer: each service's role, health and status
// word, the project column's groups, and the numbers the project pages show. Pure functions of the
// answer, so every page agrees on what "broken" means.
import type { Schemas } from "@/api/client"

export type Pod = Schemas["PodSummary"]
export type Service = Schemas["Service"]
export type Endpoint = Schemas["Endpoint"]
export type Route = Schemas["Route"]
/** Where a service's endpoints and routes are looked up: the project document lists each
 * Kubernetes Service and Ingress once, and a service names its own (`endpointNames`,
 * `routeNames`). */
export type Network = Pick<Schemas["Project"], "endpoints" | "routes">
export type Event = Schemas["EventView"]
export type Health = Schemas["Health"]
export type Role = "web" | "internal" | "worker" | "job" | "data"

const roles: Role[] = ["web", "internal", "worker", "job", "data"]

export const serviceId = (service: { kind: string; name: string }) =>
  `${service.kind}/${service.name}`

const workloadKinds: Schemas["WorkloadKind"][] = [
  "CronJob",
  "DaemonSet",
  "Deployment",
  "Job",
  "StatefulSet",
]

/** A workload kind as the API spells it (`deployment` is `Deployment`); the API reads any case, so
 * links people type or copy from kubectl work. An unknown kind is returned as it is. */
export const canonicalKind = (kind: string): string =>
  workloadKinds.find((known) => known.toLowerCase() === kind.toLowerCase()) ?? kind

/** The image's tag (`2.1`), the start of its digest, or `latest`. */
export function imageTag(image?: string | null): string {
  if (!image) return ""
  const last = image.split("/").at(-1) ?? image
  const at = last.indexOf("@")
  if (at >= 0) return last.slice(at + 1, at + 13)
  const colon = last.indexOf(":")
  return colon >= 0 ? last.slice(colon + 1) : "latest"
}

/** The PersistentVolumeClaims a service mounts, once each. */
export function volumesOf(service: Service): Schemas["Mount"][] {
  const seen = new Set<string>()
  const out: Schemas["Mount"][] = []
  for (const container of service.containers) {
    for (const mount of container.mounts) {
      if (mount.kind !== "PersistentVolumeClaim") continue
      const key = claimOf(mount)
      if (seen.has(key)) continue
      seen.add(key)
      out.push(mount)
    }
  }
  return out
}

export const claimOf = (mount: Schemas["Mount"]) =>
  mount.object?.name ?? mount.claimName ?? mount.name

/** Web: exposed by an Ingress (the server names an Ingress only when it routes to one of the
 * service's Kubernetes Services). Data: a data store, as the server judges it (`dataStore`). Job:
 * a CronJob or Job. Internal: behind a Kubernetes Service. Worker: everything else. */
export function roleOf(service: Service): Role {
  if (service.routeNames.length > 0) return "web"
  if (service.dataStore) return "data"
  if (service.kind === "CronJob" || service.kind === "Job") return "job"
  if (service.endpointNames.length > 0) return "internal"
  return "worker"
}

/** Each list's entries by name, built once per list the document holds. */
const byName = new WeakMap<readonly { name: string }[], Map<string, { name: string }>>()

function named<T extends { name: string }>(list: readonly T[], names: readonly string[]): T[] {
  let index = byName.get(list) as Map<string, T> | undefined
  if (!index) {
    index = new Map(list.map((item) => [item.name, item]))
    byName.set(list, index)
  }
  return names.flatMap((name) => index.get(name) ?? [])
}

/** The Kubernetes Services selecting the service's pods, in the order it names them. */
export const endpointsOf = (network: Network, service: Service): Endpoint[] =>
  named(network.endpoints, service.endpointNames)

/** The Ingresses routing to those Services, in the order the service names them. */
export const routesOf = (network: Network, service: Service): Route[] =>
  named(network.routes, service.routeNames)

// ── Pods ─────────────────────────────────────────────────────────────────────────────────────────

/** Why a pod is not running or ready. */
export const podReason = (pod: Pod): string | undefined => pod.reason ?? undefined

/** A pod's health, as the server judges it. */
export const podHealth = (pod: Pod): Health => pod.health

/** The pod to look at first: a failing one, else one not ready, else the first. */
export function worstPod(service: Service): Pod | undefined {
  return (
    service.pods.find((pod) => pod.health === "fail") ??
    service.pods.find((pod) => !pod.ready && pod.phase !== "Succeeded") ??
    service.pods[0]
  )
}

export const restartsOf = (service: Service) =>
  service.pods.reduce((sum, pod) => sum + pod.restarts, 0)

// ── Status ───────────────────────────────────────────────────────────────────────────────────────

type StatusWord = Schemas["StatusWord"]

export interface Status {
  health: Health
  word: StatusWord
  /** The failing pod's reason as the kubelet gives it (`CrashLoopBackOff`): in the tooltip, and
   * as a plain word (`reasonWord`) in place of the status word. */
  reason?: string
  /** A task (Job, CronJob): a run that ends with an error failed, rather than crashed. */
  task?: boolean
  ready?: number
  desired?: number
  /** The pod the reason is about, for the tooltip. */
  pod?: PodFacts
}

/** What the tooltip says of the pod a status is about: running or not, how often it restarted,
 * and how its last run that failed ended. */
export interface PodFacts {
  /** Ready now, though the reason says it crashed recently. */
  running: boolean
  restarts: number
  lastExit?: { code: number; finished?: string }
}

/** A raw reason's plain word (research: the status pill says what it means; the raw word stays in
 * its tooltip and the details). */
export type ReasonWord =
  | "crashing"
  | "runFailed"
  | "outOfMemory"
  | "imagePull"
  | "imageName"
  | "configMissing"
  | "cantStart"
  | "waitingForRoom"
  | "starting"
  | "evicted"
  | "releaseStuck"
  | "gaveUp"

const reasonWords: Record<string, ReasonWord> = {
  CrashLoopBackOff: "crashing",
  OOMKilled: "outOfMemory",
  ErrImagePull: "imagePull",
  ImagePullBackOff: "imagePull",
  InvalidImageName: "imageName",
  CreateContainerConfigError: "configMissing",
  ContainerCannotRun: "cantStart",
  RunContainerError: "cantStart",
  StartError: "cantStart",
  Unschedulable: "waitingForRoom",
  ContainerCreating: "starting",
  PodInitializing: "starting",
  Evicted: "evicted",
  ProgressDeadlineExceeded: "releaseStuck",
  BackoffLimitExceeded: "gaveUp",
}

/** The plain word for a raw reason, or undefined when there is none and the status word stands.
 * `Error` is how a run ended: a task's run failed, a service that keeps restarting crashed. */
export function reasonWord(reason: string | undefined, task: boolean): ReasonWord | undefined {
  if (!reason) return undefined
  if (reason === "Error") return task ? "runFailed" : "crashing"
  return reasonWords[reason]
}

export const isTask = (kind: string) => kind === "Job" || kind === "CronJob"

/** The facts of the pod a failing or waiting status is about. */
export function podFacts(pod: Pod): PodFacts {
  const ended = (pod.containerStatuses ?? [])
    .map((c) => c.lastTermination)
    .filter((t): t is NonNullable<typeof t> => !!t && t.exitCode !== 0)
    .toSorted((a, b) => (b.finished ?? "").localeCompare(a.finished ?? ""))[0]
  return {
    running: pod.ready && pod.phase === "Running",
    restarts: pod.restarts,
    lastExit: ended ? { code: ended.exitCode, finished: ended.finished ?? undefined } : undefined,
  }
}

/** A service's status as the server judges it (`crates/zeabur-graph/src/health.rs`), with its
 * replica counts; a CronJob between runs has none. */
export function statusOf(service: Service): Status {
  const { health, word, reason } = service.status
  const counts =
    word === "scheduled"
      ? {}
      : { ready: service.replicas?.ready, desired: service.replicas?.desired }
  const pod = health === "fail" || health === "warn" ? worstPod(service) : undefined
  return {
    health,
    word,
    reason: reason ?? undefined,
    task: isTask(service.kind),
    ...counts,
    ...(pod ? { pod: podFacts(pod) } : {}),
  }
}

/** The worst health in a project's service counts (the project list's `health`), as
 * `worstHealth` gives it for the same services. */
export function countsHealth(counts: Schemas["ProjectHealth"]): Health {
  if (counts.failing > 0) return "fail"
  if (counts.warning > 0) return "warn"
  if (counts.progressing > 0) return "progress"
  return "ok"
}

export const isHealthy = (status: Status) => status.health === "ok" || status.health === "idle"
export const needsAttention = (status: Status) =>
  status.health === "fail" || status.health === "warn"

export const healthRank: Record<Health, number> = { fail: 0, warn: 1, progress: 2, ok: 3, idle: 4 }

/** Worst first, then by name. */
export function byHealth(a: Service, b: Service): number {
  return (
    healthRank[statusOf(a).health] - healthRank[statusOf(b).health] || a.name.localeCompare(b.name)
  )
}

function worstHealth(statuses: Status[]): Health {
  let worst: Health = "ok"
  for (const status of statuses) {
    if (status.health === "idle") continue
    if (healthRank[status.health] < healthRank[worst]) worst = status.health
  }
  return worst
}

// ── Domains and configuration ────────────────────────────────────────────────────────────────────

export interface Domain {
  host: string
  ingress: string
  /** Listed in the Ingress's TLS section: HTTPS is configured (not that the certificate is valid). */
  https: boolean
  service: Service
}

/** An Ingress backend that reaches one of the service's Kubernetes Services. */
export interface PublicRoute {
  /** Absent: every host (the default backend, or a rule without a host). */
  host?: string
  path?: string
  pathType?: string
  ingress: string
  /** The Kubernetes Service. */
  backend: string
  /** A port number or name, as the Ingress writes it. */
  port?: string
  https: boolean
}

/** The Ingress backends that reach the service, in Ingress order. An Ingress lists rules for
 * other services too; only those whose backend is one of this service's Services count. */
export function publicRoutesOf(network: Network, service: Service): PublicRoute[] {
  const mine = new Set(service.endpointNames)
  return routesOf(network, service).flatMap((route) =>
    route.rules
      .filter((rule) => mine.has(rule.service))
      .map((rule) => ({
        host: rule.host ?? undefined,
        path: rule.path ?? undefined,
        pathType: rule.pathType ?? undefined,
        ingress: route.name,
        backend: rule.service,
        port: rule.port ?? undefined,
        https: rule.host ? (route.tlsHosts?.includes(rule.host) ?? false) : false,
      })),
  )
}

/** The hosts that reach the service, each once. */
export function domainsOf(network: Network, service: Service): Domain[] {
  const seen = new Map<string, Domain>()
  for (const route of publicRoutesOf(network, service))
    if (route.host && !seen.has(route.host))
      seen.set(route.host, {
        host: route.host,
        ingress: route.ingress,
        https: route.https,
        service,
      })
  return [...seen.values()]
}

export const hostUrl = (domain: Pick<Domain, "host" | "https">) =>
  `${domain.https ? "https" : "http"}://${domain.host}`

/** The Secrets and ConfigMaps a service reads values or files from. */
export function configObjectsOf(
  service: Service,
): { kind: "Secret" | "ConfigMap"; name: string }[] {
  const seen = new Map<string, { kind: "Secret" | "ConfigMap"; name: string }>()
  const add = (kind: string, name?: string | null) => {
    if ((kind === "Secret" || kind === "ConfigMap") && name)
      seen.set(`${kind}/${name}`, { kind, name })
  }
  for (const container of service.containers) {
    for (const env of container.env) add(env.source.kind, env.source.name)
    for (const imported of container.importedEnv) add(imported.source.kind, imported.source.name)
    for (const mount of container.mounts)
      if (mount.object) add(mount.object.kind, mount.object.name)
  }
  return [...seen.values()]
}

// ── Project column groups ────────────────────────────────────────────────────────────────────────

export interface ServiceGroup {
  key: string
  /** A role (translated by the caller), or the `part-of` group name as written. */
  role?: Role
  name?: string
  services: Service[]
}

/** By `app.kubernetes.io/part-of` when the project uses more than one group, else by role. */
export function groupServices(services: Service[]): ServiceGroup[] {
  const named = new Set(services.map((s) => s.group).filter((g) => g && g !== "default"))
  if (named.size > 1) {
    const groups = new Map<string, Service[]>()
    for (const service of services) {
      const key = service.group || "default"
      groups.set(key, [...(groups.get(key) ?? []), service])
    }
    return [...groups.entries()].map(([key, list]) => ({
      key,
      name: key,
      services: list.toSorted((a, b) => a.name.localeCompare(b.name)),
    }))
  }
  return roles
    .map((role) => ({
      key: role,
      role,
      services: services
        .filter((service) => roleOf(service) === role)
        .toSorted((a, b) => a.name.localeCompare(b.name)),
    }))
    .filter((group) => group.services.length > 0)
}

// ── Project summary ──────────────────────────────────────────────────────────────────────────────

export type Project = Schemas["Project"]

export interface ProjectSummary {
  services: number
  healthy: number
  podsReady: number
  pods: number
  restarts: number
  domains: Domain[]
  health: Health
}

export function summarise(project: Project): ProjectSummary {
  const statuses = project.services.map(statusOf)
  const pods = project.services.flatMap((s) => s.pods)
  return {
    services: project.services.length,
    healthy: statuses.filter(isHealthy).length,
    podsReady: pods.filter((p) => p.ready).length,
    pods: pods.length,
    restarts: pods.reduce((sum, p) => sum + p.restarts, 0),
    domains: project.services.flatMap((service) => domainsOf(project, service)),
    health: worstHealth(statuses),
  }
}

export interface Claim {
  name: string
  size?: string | null
  sizeBytes?: number | null
  status?: string | null
  storageClass?: string | null
  /** The services mounting it. */
  services: Service[]
  orphan: boolean
}

/** Every PersistentVolumeClaim of the project, with the services that mount it. */
export function claimsOf(project: Project): Claim[] {
  const claims = new Map<string, Claim>()
  for (const service of project.services)
    for (const mount of volumesOf(service)) {
      const name = claimOf(mount)
      const claim = claims.get(name) ?? {
        name,
        size: mount.size,
        sizeBytes: mount.sizeBytes,
        status: mount.status,
        storageClass: mount.storageClass,
        services: [],
        orphan: false,
      }
      claim.services.push(service)
      claims.set(name, claim)
    }
  for (const resource of project.resources)
    if (resource.kind === "PersistentVolumeClaim" && !claims.has(resource.name))
      claims.set(resource.name, {
        name: resource.name,
        services: [],
        orphan: resource.attribution.ownership === "orphan",
      })
  return [...claims.values()].toSorted((a, b) => a.name.localeCompare(b.name))
}

// ── Events ───────────────────────────────────────────────────────────────────────────────────────

export const newestFirst = (a: Event, b: Event) =>
  (b.lastSeen ?? "").localeCompare(a.lastSeen ?? "")

/** Events about a service: the workload itself or one of its pods. */
export function eventsFor(events: Event[], service: Service): Event[] {
  const pods = new Set(service.pods.map((p) => p.name))
  return events.filter(
    (event) =>
      (event.regarding?.kind === service.kind && event.regarding.name === service.name) ||
      (event.regarding?.kind === "Pod" && pods.has(event.regarding.name ?? "")),
  )
}

export const isWarning = (event: Event) => event.type === "Warning"
