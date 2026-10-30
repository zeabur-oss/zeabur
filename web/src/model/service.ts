import type { Schemas } from "@/api/client"

export type Pod = Schemas.PodSummary
export type Service = Schemas.Service
export type Endpoint = Schemas.Endpoint
export type Route = Schemas.Route
export type Network = Pick<Schemas.Project, "endpoints" | "routes">
export type Event = Schemas.EventView
export type Health = Schemas.Health
export type Role = "web" | "internal" | "worker" | "job" | "data"

const roles: Role[] = ["web", "internal", "worker", "job", "data"]

export const serviceId = (service: { kind: string; name: string }) =>
  `${service.kind}/${service.name}`

const workloadKinds: Schemas.WorkloadKind[] = [
  "CronJob",
  "DaemonSet",
  "Deployment",
  "Job",
  "StatefulSet",
]

/** The API reads any case, so links typed or copied from kubectl work; an unknown kind is kept. */
export const canonicalKind = (kind: string): string =>
  workloadKinds.find((known) => known.toLowerCase() === kind.toLowerCase()) ?? kind

export function imageTag(image?: string | null): string {
  if (!image) return ""
  const last = image.split("/").at(-1) ?? image
  const at = last.indexOf("@")
  if (at >= 0) return last.slice(at + 1, at + 13)
  const colon = last.indexOf(":")
  return colon >= 0 ? last.slice(colon + 1) : "latest"
}

export function volumesOf(service: Service): Schemas.Mount[] {
  const seen = new Set<string>()
  const out: Schemas.Mount[] = []
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

export const claimOf = (mount: Schemas.Mount) => mount.object?.name ?? mount.claimName ?? mount.name

export function roleOf(service: Service): Role {
  if (service.routeNames.length > 0) return "web"
  if (service.dataStore) return "data"
  if (service.kind === "CronJob" || service.kind === "Job") return "job"
  if (service.endpointNames.length > 0) return "internal"
  return "worker"
}

const byName = new WeakMap<readonly { name: string }[], Map<string, { name: string }>>()

function named<T extends { name: string }>(list: readonly T[], names: readonly string[]): T[] {
  let index = byName.get(list) as Map<string, T> | undefined
  if (!index) {
    index = new Map(list.map((item) => [item.name, item]))
    byName.set(list, index)
  }
  return names.flatMap((name) => index.get(name) ?? [])
}

/** The port placeholder in {@link exposeCommand} when the port is not known. */
export const PORT_PLACEHOLDER = "<port>"

/** The kubectl command that gives a Deployment a Service. A container that declares no port needs
 * `--port`: it is the readiness probe's port when that is a number, else {@link PORT_PLACEHOLDER}. */
export function exposeCommand(service: Service): string | undefined {
  if (service.kind !== "Deployment") return undefined
  const port = service.containers
    .filter((c) => !c.init)
    .map((c) => c.probes?.readiness?.port)
    .find((p) => p && /^\d+$/.test(p))
  return `kubectl -n ${service.namespace} expose deployment ${service.name} --port=${port ?? PORT_PLACEHOLDER}`
}

export const endpointsOf = (network: Network, service: Service): Endpoint[] =>
  named(network.endpoints, service.endpointNames)

export const routesOf = (network: Network, service: Service): Route[] =>
  named(network.routes, service.routeNames)

export const podReason = (pod: Pod): string | undefined => pod.reason ?? undefined

export const podHealth = (pod: Pod): Health => pod.health

export function worstPod(service: Pick<Service, "pods">): Pod | undefined {
  return (
    service.pods.find((pod) => pod.health === "fail") ??
    service.pods.find((pod) => !pod.ready && pod.phase !== "Succeeded") ??
    service.pods[0]
  )
}

export const restartsOf = (service: Service) =>
  service.pods.reduce((sum, pod) => sum + pod.restarts, 0)

type StatusWord = Schemas.StatusWord

export interface Status {
  health: Health
  word: StatusWord
  /** The kubelet's reason (`CrashLoopBackOff`). */
  reason?: string
  /** Job or CronJob: a run that ends with an error failed, rather than crashed. */
  task?: boolean
  ready?: number
  desired?: number
  pod?: PodFacts
  /** Ready, but no container has a readiness probe: started, not shown to answer. */
  unchecked?: boolean
}

export interface PodFacts {
  /** Ready now, though the reason says it crashed recently. */
  running: boolean
  restarts: number
  lastExit?: { code: number; finished?: string }
}

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
  | "buildFailed"
  | "security"

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
  BuildFailed: "buildFailed",
}

/** `Error` is how a run ended: a task's run failed, a service that keeps restarting crashed. */
export function reasonWord(reason: string | undefined, task: boolean): ReasonWord | undefined {
  if (!reason) return undefined
  if (reason === "Error") return task ? "runFailed" : "crashing"
  return reasonWords[reason]
}

export const isTask = (kind: string) => kind === "Job" || kind === "CronJob"

function podFacts(pod: Pod): PodFacts {
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

/** Mirrors `crates/zeabur-graph/src/health.rs`; a CronJob between runs has no replica counts. */
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
    ...(word === "ready" && !hasReadinessCheck(service) ? { unchecked: true } : {}),
  }
}

/** Whether a long-running service's readiness is checked by a probe; a task's always counts. */
function hasReadinessCheck(service: Service): boolean {
  if (isTask(service.kind)) return true
  const main = service.containers.filter((c) => !c.init)
  return main.length === 0 || main.some((c) => !!c.probes?.readiness)
}

/** Must agree with `worstHealth` over the same services. */
export function countsHealth(counts: Schemas.ProjectHealth): Health {
  if (counts.failing > 0) return "fail"
  if (counts.warning > 0) return "warn"
  if (counts.progressing > 0) return "progress"
  return "ok"
}

export const isHealthy = (status: Status) => status.health === "ok" || status.health === "idle"
export const needsAttention = (status: Status) =>
  status.health === "fail" || status.health === "warn"

export const healthRank: Record<Health, number> = { fail: 0, warn: 1, progress: 2, ok: 3, idle: 4 }

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

export interface Domain {
  host: string
  ingress: string
  /** Listed in the Ingress's TLS section; says nothing of the certificate's validity. Unknown
   * for an HTTPRoute, as the Gateway's listener decides. */
  https?: boolean
  service: Service
}

export interface PublicRoute {
  /** Absent for the default backend or a rule without a host. */
  host?: string
  path?: string
  pathType?: string
  ingress: string
  backend: string
  port?: string
  /** Unknown for an HTTPRoute, as the Gateway's listener decides. */
  https?: boolean
}

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
        https:
          route.origin.kind === "HTTPRoute"
            ? undefined
            : rule.host
              ? (route.tlsHosts?.includes(rule.host) ?? false)
              : false,
      })),
  )
}

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
  `${domain.https === false ? "http" : "https"}://${domain.host}`

/** The Secrets a service's variables read with `env` or `envFrom`; mounted ones are files. */
export function envSecretsOf(service: Service): string[] {
  const names = new Set<string>()
  for (const container of service.containers)
    for (const { source } of [...container.env, ...container.importedEnv])
      if (source.kind === "Secret" && source.name) names.add(source.name)
  return [...names]
}

export interface ServiceGroup {
  key: string
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

export type Project = Schemas.Project

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
  services: Service[]
  orphan: boolean
}

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

export const newestFirst = (a: Event, b: Event) =>
  (b.lastSeen ?? "").localeCompare(a.lastSeen ?? "")

/** About the workload itself or one of its pods. */
export function eventsFor(events: Event[], service: Service): Event[] {
  const pods = new Set(service.pods.map((p) => p.name))
  return events.filter(
    (event) =>
      (event.regarding?.kind === service.kind && event.regarding.name === service.name) ||
      (event.regarding?.kind === "Pod" && pods.has(event.regarding.name ?? "")),
  )
}

export const isWarning = (event: Event) => event.type === "Warning"
