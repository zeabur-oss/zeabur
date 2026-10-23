import type { Schemas } from "@/api/client"
import type { Service } from "@/model/service"

export type Build = Schemas["ZeaburBuilderBuild"]
export type BuildRun = Schemas["ProjectBuildRun"]
export type Builds = Schemas["ProjectBuilds"]

/** Where Zeabur tells Keel of new images: the catalog's Keel, by its Service. */
export const KEEL_WEBHOOK = "http://keel.keel.svc.cluster.local:9300/v1/webhooks/native"

/**
 * An image's repository as a registry names it: no tag or digest, Docker Hub's host and
 * `library/` spelt out, lower case.
 */
export function repositoryOf(image: string): string {
  let name = image.trim().toLowerCase()
  const at = name.indexOf("@")
  if (at >= 0) name = name.slice(0, at)
  const slash = name.lastIndexOf("/")
  const colon = name.lastIndexOf(":")
  if (colon > slash) name = name.slice(0, colon)
  const [first = "", ...rest] = name.split("/")
  const host =
    rest.length > 0 && (first.includes(".") || first.includes(":") || first === "localhost")
  if (host) return first === "index.docker.io" ? `docker.io/${rest.join("/")}` : name
  return rest.length === 0 ? `docker.io/library/${name}` : `docker.io/${name}`
}

const digestOf = (image: string) => image.match(/@(sha256:[0-9a-f]{64})$/)?.[1]
const tagOf = (image: string) => {
  const name = image.split("@")[0] ?? image
  const slash = name.lastIndexOf("/")
  const colon = name.lastIndexOf(":")
  return colon > slash ? name.slice(colon + 1) : undefined
}

/** A container and the Build whose repository its image is in. */
export interface Built {
  container: string
  image: string
  build: Build
}

/** The service's containers that run a Build's image, by repository; init containers too. */
export function builtContainers(service: Service, builds: readonly Build[]): Built[] {
  const out: Built[] = []
  for (const container of service.containers) {
    if (!container.image) continue
    const repository = repositoryOf(container.image)
    const build = builds.find((b) => b.repository && repositoryOf(b.repository) === repository)
    if (build) out.push({ container: container.name, image: container.image, build })
  }
  return out
}

/** The Builds that make any of the service's images, each once. */
export function buildsOf(service: Service, builds: readonly Build[] | undefined): Build[] {
  const found = builtContainers(service, builds ?? []).map((b) => b.build)
  return found.filter((b, i) => found.findIndex((other) => other.name === b.name) === i)
}

/** Whether `image` is the Build's newest: the same digest, or the same tag when it has none. */
export function runsLatest(image: string, build: Build): boolean | undefined {
  const latest = build.latestImage
  if (!latest) return undefined
  const have = digestOf(image)
  const want = digestOf(latest)
  if (have && want) return have === want
  return tagOf(image) !== undefined && tagOf(image) === tagOf(latest)
}

/** The Build's runs, newest first. */
export const runsOf = (runs: readonly BuildRun[], build: Build) =>
  runs.filter((run) => run.build === build.name)

export type RunTone = "ok" | "fail" | "progress" | "neutral"

export function runTone(run: BuildRun): RunTone {
  switch (run.phase) {
    case "Succeeded":
      return "ok"
    case "Failed":
      return "fail"
    case "Pending":
    case "Planning":
    case "Building":
      return "progress"
    default:
      return "neutral"
  }
}

const FINISHED = new Set(["Succeeded", "Failed", "Cancelled"])

/** The branch the Build builds: what its default branch resolved to, or the ref it names. */
export function branchOf(build: Build): string | undefined {
  const named = build.gitRef && build.gitRef !== "HEAD" ? build.gitRef : undefined
  const name = (build.resolvedRef ?? named)?.replace(/^refs\/(heads|tags)\//, "")
  return name === "HEAD" ? undefined : name
}

/** Why the Build cannot build, unless a run is under way. */
export function buildProblem(build: Build, latest: BuildRun | undefined) {
  if (latest && (!latest.phase || !FINISHED.has(latest.phase))) return undefined
  return build.problem ?? undefined
}

/**
 * Whether a build is under way or about to be: a run not finished yet, a Build with no image, run
 * or problem yet, or a request to build again, at most two minutes old, that no run has answered
 * yet.
 */
export function building(data: Builds, now: number): boolean {
  if (data.runs.some((run) => !run.phase || !FINISHED.has(run.phase))) return true
  return data.builds.some((build) => {
    if (!build.latestImage && !build.problem && !data.runs.some((r) => r.build === build.name))
      return true
    const asked = build.rebuildRequested ? Date.parse(build.rebuildRequested) : Number.NaN
    if (!Number.isFinite(asked) || now - asked > 120_000) return false
    const newest = data.runs.find((run) => run.build === build.name)
    return !newest?.created || Date.parse(newest.created) < asked
  })
}

/** Milliseconds from start to finish, or to `now` while it runs. */
export function durationOf(run: BuildRun, now: number): number | undefined {
  if (!run.started) return undefined
  const end = run.finished ? Date.parse(run.finished) : now
  const ms = end - Date.parse(run.started)
  return Number.isFinite(ms) && ms >= 0 ? ms : undefined
}

/** Whether Keel updates the workload: any `keel.sh/policy` but `never`. */
export function keelPolicy(builds: Builds, service: Service): string | undefined {
  const policy = builds.keel.find((k) => k.kind === service.kind && k.name === service.name)?.policy
  return policy && policy !== "never" ? policy : undefined
}

/** A commit's short form. */
export const shortSha = (sha?: string | null) => (sha ? sha.slice(0, 7) : undefined)

const BUILD_RESOURCE = { group: "builder.zeabur.com", version: "v1alpha1", resource: "builds" }
const SECRET_RESOURCE = { version: "v1", resource: "secrets" }

/**
 * What deleting `service` deletes besides its workload: each Build whose image no other service
 * runs, and the `<build>-git` Secret "Deploy from Git" made for it when no other Build reads it.
 */
export function buildDeletions(
  service: Service,
  services: readonly Service[],
  builds: readonly Build[],
): Schemas["Change"][] {
  const others = services.filter((s) => s.kind !== service.kind || s.name !== service.name)
  const gone = [...new Set(builtContainers(service, builds).map((b) => b.build))].filter((build) =>
    others.every((s) => builtContainers(s, [build]).length === 0),
  )
  const kept = builds.filter((b) => !gone.includes(b))
  return gone.flatMap((build) => [
    { op: "delete" as const, resource: BUILD_RESOURCE, name: build.name },
    ...(build.credentials === `${build.name}-git` &&
    kept.every((b) => b.credentials !== build.credentials)
      ? [{ op: "delete" as const, resource: SECRET_RESOURCE, name: build.credentials }]
      : []),
  ])
}

/** Which credentials the Git host refused, when it refused them: a deploy key over SSH or a
 * token over HTTPS. */
export function refusedCredentials(build: Build, latest: BuildRun | undefined) {
  const problem = buildProblem(build, latest)
  if (problem?.reason !== "AuthFailed" || !build.credentials) return undefined
  return /^(ssh:\/\/|[\w.-]+@[^:/]+:)/.test(build.gitUrl ?? "") ? "deployKey" : "token"
}
