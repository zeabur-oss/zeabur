// A service from a Git repository: the form, sent as one `createBuiltService` change. The server
// makes Zeabur Builder's Build and the objects of the service that runs what it builds.
import type { Schemas } from "@/api/client"
import { rules } from "@/api/rules"
import type { Build } from "@/model/builds"
import { textProblem } from "@/model/rules"
import {
  hostProblem,
  portProblem,
  serviceNameProblem,
  type ImageService,
} from "@/model/image-service"
import type { Profile } from "@/model/pod-security"

type Change = Schemas.Change
type EnvEdit = Schemas.EnvEdit

export type BuildMethod = Schemas.BuildMethod
export const buildMethods: BuildMethod[] = ["railpack", "dockerfile"]

/** The methods to offer: the agent's only while its builder is ready, or when `current` is it. */
export const offeredMethods = (agentReady: boolean, current?: BuildMethod): BuildMethod[] =>
  agentReady || current === "agent" ? [...buildMethods, "agent"] : buildMethods

/** `githubApp`: the cluster's GitHub App, through an installation granted to the project. */
export type Access = "public" | "deployKey" | "token" | "githubApp"

/** Hosts whose SSH host keys Zeabur pins; any other host takes the person's `known_hosts`. */
const PINNED_HOSTS = ["github.com", "gitlab.com"]

export interface Repository {
  host: string
  /** `owner/repo`, or deeper for GitLab groups; no `.git`. */
  path: string
}

export interface GitService extends Pick<
  ImageService,
  "name" | "port" | "publicUrl" | "host" | "size" | "security"
> {
  variables: Schemas.NameValue[]
  url: string
  /** Empty: the repository's default branch. */
  branch: string
  /** Empty: the repository's root. */
  directory: string
  method: BuildMethod
  /** For `dockerfile`, relative to `directory`. Empty: `Dockerfile`. */
  dockerfile: string
  /** Passed to the build as BuildKit secrets, from the Secret `<name>-build-env`. */
  buildVariables: { name: string; value: string }[]
  access: Access
  /** For `token`. */
  username: string
  token: string
  /** For `deployKey` on a host Zeabur does not pin: its `known_hosts` lines. */
  knownHosts: string
  /** For `githubApp`. */
  installationId?: number
}

/** What Railpack's apps listen on once Zeabur sets `PORT`; the form starts with it. */
export const DEFAULT_BUILT_PORT = "8080"

/** The method the form builds with: the person's once they choose, else Dockerfile when the
 * folder has one at the path. */
export const derivedMethod = (chosen: boolean, method: BuildMethod, dockerfileFound: boolean) =>
  chosen ? method : dockerfileFound ? "dockerfile" : "railpack"

/** The port a service listens on before the person types one: Railpack's, and none for a
 * Dockerfile (no network). */
export const defaultPort = (method: BuildMethod) =>
  method === "railpack" ? DEFAULT_BUILT_PORT : ""

/** Where the Dockerfile is in the repository or folder: `dockerfile` is relative to `directory`. */
export const dockerfileAt = (directory: string, dockerfile: string) =>
  [cleanPath(directory), cleanPath(dockerfile) || "Dockerfile"].filter(Boolean).join("/")

export const newGitService = (): GitService => ({
  url: "",
  branch: "",
  directory: "",
  method: "railpack",
  dockerfile: "",
  buildVariables: [],
  access: "public",
  username: "",
  token: "",
  knownHosts: "",
  name: "",
  port: DEFAULT_BUILT_PORT,
  publicUrl: true,
  host: "",
  variables: [],
  size: "small",
  security: "restricted",
})

/** The profile a build's image suits: Railpack's images run as user 1000, a Dockerfile's as it
 * says, often root. */
export const buildProfile = (method: BuildMethod): Profile =>
  method === "railpack" ? "restricted" : "baseline"

const SEGMENT = /^[A-Za-z0-9._-]+$/
const BRANCH = new RegExp(rules["BuildSource.git.branch"].pattern)
const HOST = /^[a-z0-9]([-a-z0-9.]*[a-z0-9])?(:\d{1,5})?$/

/** The host and path of `https://host/owner/repo(.git)`, `git@host:owner/repo.git` or
 * `ssh://git@host/owner/repo.git`; nothing for anything else. */
export function parseRepository(url: string): Repository | undefined {
  const value = url.trim()
  let host: string
  let path: string
  const scp = /^[A-Za-z0-9._-]+@([^:/]+):(?!\/)(.+)$/.exec(value)
  if (scp) {
    host = scp[1] ?? ""
    path = scp[2] ?? ""
  } else {
    let parsed: URL
    try {
      parsed = new URL(value)
    } catch {
      return undefined
    }
    if (!["https:", "http:", "ssh:"].includes(parsed.protocol)) return undefined
    if (parsed.protocol !== "ssh:" && (parsed.username || parsed.password)) return undefined
    host = parsed.host
    path = parsed.pathname.replace(/^\/+/, "")
  }
  host = host.toLowerCase()
  path = path.replace(/\/+$/, "").replace(/\.git$/, "")
  const segments = path.split("/")
  if (!HOST.test(host) || segments.length < 2 || !segments.every((s) => SEGMENT.test(s)))
    return undefined
  return { host, path }
}

const overHttps = (url: string) => /^https?:/i.test(url.trim())

/** The SSH host a deploy key for `url` is for, with `:port` from an `ssh://` URL. */
export function deployKeyHost(url: string): string | undefined {
  const repo = parseRepository(url)
  if (!repo) return undefined
  return overHttps(url) ? repo.host.replace(/:\d+$/, "") : repo.host
}

/** Zeabur knows the host key, so the person need not give it. */
export const pinnedHost = (host: string) => PINNED_HOSTS.includes(host)

/** The command that prints `host`'s keys in the types Zeabur Builder takes. */
export function keyscanCommand(host: string): string {
  const [name, port] = host.split(":")
  return `ssh-keyscan -t ed25519,ecdsa ${port ? `-p ${port} ` : ""}${name}`
}

/** Where the host takes a deploy key for the repository. */
export function deployKeyPage(repo: Repository): string | undefined {
  if (repo.host === "github.com") return `https://github.com/${repo.path}/settings/keys/new`
  if (repo.host === "gitlab.com")
    return `https://gitlab.com/${repo.path}/-/settings/repository#js-deploy-keys-settings`
  return undefined
}

/** The user name a host takes with an access token as the password. */
export function tokenUser(host: string | undefined): string {
  if (host === "github.com") return "x-access-token"
  if (host === "gitlab.com") return "oauth2"
  return ""
}

export function urlProblem(url: string): "empty" | "invalid" | undefined {
  if (url.trim() === "") return "empty"
  return parseRepository(url) ? undefined : "invalid"
}

export function branchProblem(branch: string): "invalid" | undefined {
  const value = branch.trim()
  if (value === "") return undefined
  return BRANCH.test(value) ? undefined : "invalid"
}

/** A path inside the repository as the Build takes it: no `./` in front, no `/` behind. */
const cleanPath = (path: string) =>
  path
    .trim()
    .replace(/^\.?\/+/, "")
    .replace(/\/+$/, "")

/** What is wrong with a directory or Dockerfile path, by the server's rule; empty is fine. */
export function pathProblem(path: string): "long" | "invalid" | undefined {
  return textProblem(cleanPath(path), rules["BuildOptionsChange.contextDir"])
}

/** What is wrong with the fields every service built by Zeabur Builder has. */
export function builtProblems(
  form: BuiltService,
  {
    taken,
    badEnv,
    badBuildEnv,
  }: { taken: readonly string[]; badEnv: boolean; badBuildEnv: boolean },
) {
  return {
    name: serviceNameProblem(form.name, taken),
    port: portProblem(form.port),
    host: form.publicUrl ? hostProblem(form.host) : undefined,
    directory: pathProblem(form.directory),
    dockerfile: pathProblem(form.dockerfile),
    env: badEnv,
    buildEnv: badBuildEnv,
  }
}

/** What is wrong with how the form reads the repository, its access and user resolved. */
export function repositoryProblems(form: GitSource, hostChecked: boolean) {
  return {
    url: urlProblem(form.url),
    branch: branchProblem(form.branch),
    hostKeys: form.access === "deployKey" && !hostChecked,
    token: form.access === "token" && (form.token.trim() === "" || form.username.trim() === ""),
    installation: form.access === "githubApp" && !form.installationId,
  }
}

/** The Secret that holds a Build's variables. */
export const buildEnvSecret = (build: string) => `${build}-build-env`

/** What a service built by Zeabur Builder needs from its form, whatever the source. */
export type BuiltService = Pick<
  GitService,
  | "name"
  | "port"
  | "publicUrl"
  | "host"
  | "variables"
  | "size"
  | "security"
  | "method"
  | "buildVariables"
  | "directory"
  | "dockerfile"
>

/** The repository a Build reads and how it builds it: what a new Git service and a Git source
 * share. */
export type GitSource = Pick<
  GitService,
  | "url"
  | "branch"
  | "directory"
  | "method"
  | "dockerfile"
  | "access"
  | "username"
  | "token"
  | "knownHosts"
  | "installationId"
>

/** How Zeabur Builder reads the repository, as the form chose. */
function accessOf(form: GitSource): Schemas.GitAccess {
  const host = deployKeyHost(form.url)
  switch (form.access) {
    case "public":
      return { type: "public" }
    case "token":
      return { type: "token", username: form.username.trim(), token: form.token.trim() }
    case "deployKey":
      return host && pinnedHost(host)
        ? { type: "deployKey" }
        : { type: "deployKey", knownHosts: form.knownHosts }
    case "githubApp":
      return { type: "githubApp", installationId: form.installationId ?? 0 }
  }
}

/** The paths of a Build as the form gives them: the Dockerfile only for `dockerfile`. */
export function buildPaths(form: Pick<BuiltService, "method" | "directory" | "dockerfile">) {
  const directory = cleanPath(form.directory)
  const dockerfile = form.method === "dockerfile" ? cleanPath(form.dockerfile) : ""
  return {
    method: form.method,
    ...(directory ? { directory } : {}),
    ...(dockerfile ? { dockerfile } : {}),
  }
}

/** The Build the form asks for, reading the repository. */
function gitBuild(form: GitSource): Schemas.BuildInput {
  const branch = form.branch.trim()
  return {
    source: {
      type: "git",
      url: form.url.trim(),
      ...(branch ? { branch } : {}),
      access: accessOf(form),
    },
    ...buildPaths(form),
  }
}

/** The service fields of a new built service, with its `build`. */
export function builtServiceChange(
  form: BuiltService,
  build: Schemas.BuildInput,
): Schemas.BuiltServiceChange {
  const port = form.port.trim()
  return {
    name: form.name,
    ...(port ? { port: Number(port) } : {}),
    ...(port && form.publicUrl ? { host: form.host.trim() } : {}),
    variables: form.variables,
    size: form.size,
    security: form.security,
    build: { ...build, buildVariables: form.buildVariables },
  }
}

/** The change the form sends. */
export const gitChange = (form: GitService): Schemas.BuiltServiceChange =>
  builtServiceChange(form, gitBuild(form))

export const createGitService = (form: GitService): Change[] => [
  { op: "createBuiltService", ...gitChange(form) },
]

export const newGitSource = (method: BuildMethod = "railpack"): GitSource => ({
  url: "",
  branch: "",
  directory: "",
  method,
  dockerfile: "",
  access: "public",
  username: "",
  token: "",
  knownHosts: "",
})

/** How a Build builds, by its builder. */
export const methodOf = (build: Build): BuildMethod =>
  build.builder === "dockerfile" || build.builder === "agent" ? build.builder : "railpack"

/** The branch a Build follows as the form takes it: empty for the repository's default. */
const branchField = (ref: string | null | undefined) =>
  !ref || ref === "HEAD" ? "" : ref.replace(/^refs\/(heads|tags)\//, "")

/** The form for a Build as it is: its repository, branch, paths and method. Credentials stay in
 * their Secret: an SSH address reads with its deploy key, credentials over HTTPS with a token typed
 * again; a repository the GitHub App reads resolves to the App as in "Deploy from Git". */
export function gitSourceOf(build: Build | undefined): GitSource {
  if (!build) return newGitSource()
  const url = build.gitUrl ?? ""
  const access: Access =
    url && !overHttps(url) ? "deployKey" : url && build.credentials ? "token" : "public"
  return {
    ...newGitSource(methodOf(build)),
    url,
    branch: branchField(build.gitRef),
    directory: build.contextDir ?? "",
    dockerfile: build.dockerfile ?? "",
    access,
  }
}

/** The form with the Build's paths as they are now: its options form edits them, and a change of
 * source keeps them. */
export const withPathsOf = (form: GitSource, build: Build | undefined): GitSource =>
  build ? { ...form, directory: build.contextDir ?? "", dockerfile: build.dockerfile ?? "" } : form

/** The change that builds `container` of `workload` from the repository: a new Build named after
 * the workload, or its Build reading the repository instead. */
export const gitSourceChange = (
  form: GitSource,
  workload: Schemas.WorkloadRef,
  container?: string,
): Change => ({
  op: "setBuild",
  workload,
  ...(container ? { container } : {}),
  build: gitBuild(form),
})

/** A Build's options as the service's settings edit them. */
export interface BuildOptions {
  directory: string
  dockerfile: string
  /** Variables to remove, by name. */
  removed: string[]
  /** Variables to add, or whose values to replace. */
  set: { name: string; value: string }[]
}

export const buildOptionsOf = (build: Build): BuildOptions => ({
  directory: build.contextDir ?? "",
  dockerfile: build.dockerfile ?? "",
  removed: [],
  set: [],
})

/** The changes that give `build` these options. Values go to its Secret `<build>-build-env`: a
 * `data` change when a variable already reads from it, else a new Secret. */
export function buildOptionsChanges(build: Build, options: BuildOptions): Change[] {
  const secret = buildEnvSecret(build.name)
  const setNames = new Set(options.set.map((v) => v.name))
  const wired = (name: string) =>
    build.env.some((e) => e.name === name && e.secret === secret && e.key === name)
  const env: EnvEdit[] = [
    ...options.removed
      .filter((name) => !setNames.has(name) && build.env.some((e) => e.name === name))
      .map((name): EnvEdit => ({ op: "remove", name })),
    ...options.set
      .filter((v) => !wired(v.name))
      .map((v): EnvEdit => ({
        op: "set",
        name: v.name,
        valueFrom: { secretName: secret, key: v.name },
      })),
  ]
  const directory = cleanPath(options.directory)
  const dockerfile = cleanPath(options.dockerfile)
  const changes: Change[] = []
  const edit = {
    ...(directory === (build.contextDir ?? "") ? {} : { contextDir: directory }),
    ...(dockerfile === (build.dockerfile ?? "") ? {} : { dockerfile }),
    ...(env.length > 0 ? { env } : {}),
  }
  if (Object.keys(edit).length > 0) changes.push({ op: "buildOptions", build: build.name, ...edit })
  const exists = build.env.some((e) => e.secret === secret)
  const dropped = options.removed.filter(
    (name) => !setNames.has(name) && build.env.some((e) => e.name === name && e.secret === secret),
  )
  if (exists && (options.set.length > 0 || dropped.length > 0))
    changes.push({
      op: "data",
      kind: "Secret",
      name: secret,
      changes: [
        ...dropped.map((key) => ({ op: "remove", key })),
        ...options.set.map((v) => ({ op: "set", key: v.name, value: v.value })),
      ],
    })
  else if (!exists && options.set.length > 0)
    changes.push({
      op: "create",
      object: {
        apiVersion: "v1",
        kind: "Secret",
        metadata: { name: secret, labels: { "app.kubernetes.io/name": build.name } },
        type: "Opaque",
        stringData: Object.fromEntries(options.set.map((v) => [v.name, v.value])),
      },
    })
  return changes
}

/** What "Deploy from Git" lacks in this cluster: components to install, whether Keel is missing
 * with nothing that installs it here, and whether Zeabur's chart names no registry. Unknown counts
 * as there. */
export interface GitNeeds {
  install: Schemas.ComponentRef[]
  keelUnavailable: boolean
  registrySetting: boolean
}

export function gitNeeds({
  builds,
  capabilities,
  registry,
}: {
  builds: Pick<Schemas.ProjectBuilds, "state" | "component"> | undefined
  capabilities: Schemas.Capability[] | undefined
  /** `""` when Zeabur has no registry setting; `undefined` while unknown. */
  registry: string | undefined
}): GitNeeds | undefined {
  const install: Schemas.ComponentRef[] = []
  if (builds?.state === "not-installed" && builds.component) install.push(builds.component)
  const keel = missingKeel(capabilities)
  if (keel?.install) install.push(keel.install)
  const keelUnavailable = keel !== undefined && !keel.install
  const registrySetting = registry === ""
  return install.length > 0 || keelUnavailable || registrySetting
    ? { install, keelUnavailable, registrySetting }
    : undefined
}

/** Image updates (Keel) when the cluster has none, with the component that installs it here if
 * any; `undefined` when it is there or unknown. */
export const missingKeel = (capabilities: Schemas.Capability[] | undefined) => {
  const keel = capabilities?.find((c) => c.name === "image-updates")
  return keel?.existing === false ? { install: keel.install ?? undefined } : undefined
}
