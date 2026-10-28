// A service from a Git repository: Zeabur Builder's Build, and the objects of an image service
// whose image is the Build's (docs/design: basic create). The Deployment names the Build's
// repository with a tag no build makes, and Keel moves it to each image the Build pushes.
import type { Schemas } from "@/api/client"
import { KEEL_WEBHOOK, type Build } from "@/model/builds"
import {
  hostProblem,
  imageObjects,
  portProblem,
  serviceNameProblem,
  type DomainSettings,
  type ImageService,
} from "@/model/image-service"
import type { Profile } from "@/model/pod-security"

type Change = Schemas["Change"]
type EnvEdit = Schemas["EnvEdit"]
type Manifest = Record<string, unknown>

export type BuildMethod = "railpack" | "dockerfile"
export const buildMethods: BuildMethod[] = ["railpack", "dockerfile"]

/** `githubApp`: the cluster's GitHub App, through an installation granted to the project. */
export type Access = "public" | "deployKey" | "token" | "githubApp"

/** The ref Zeabur Builder takes for the repository's default branch. */
const DEFAULT_BRANCH_REF = "HEAD"

/** The tag the Deployment names until the first build. */
const PENDING_TAG = "pending"
const GIT_CREDENTIALS_LABEL = "builder.zeabur.com/git-credentials"
/** Hosts whose SSH host keys Zeabur pins; any other host takes the person's `known_hosts`. */
const PINNED_HOSTS = ["github.com", "gitlab.com"]

export interface Repository {
  host: string
  /** `owner/repo`, or deeper for GitLab groups; no `.git`. */
  path: string
}

export interface GitService extends Pick<
  ImageService,
  "name" | "port" | "publicUrl" | "host" | "variables" | "size" | "security"
> {
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
  /** For `deployKey`: the sealed key `POST /deploy-keys` answered. */
  sealedKey?: string
  /** For `githubApp`. */
  installationId?: number
}

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
  name: "",
  port: "",
  publicUrl: false,
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
const BRANCH = /^(?!\/|.*\/\/|.*\.\.|.*\/$|.*\.lock$)[A-Za-z0-9._/-]+$/
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

const httpsUrl = (repo: Repository) => `https://${repo.host}/${repo.path}.git`
const overHttps = (url: string) => /^https?:/i.test(url.trim())

/** The repository over SSH: as typed when it is an SSH URL, else `git@host:path.git`. */
function sshUrl(url: string, repo: Repository): string {
  if (!overHttps(url)) return url.trim()
  return `git@${repo.host.replace(/:\d+$/, "")}:${repo.path}.git`
}

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
export const cleanPath = (path: string) =>
  path
    .trim()
    .replace(/^\.?\/+/, "")
    .replace(/\/+$/, "")

/** No segment may start with a dot, as the Build's schema says, so `..` cannot climb out. */
const PATH_SEGMENT = /^[A-Za-z0-9_@+-][A-Za-z0-9._@+-]*$/

/** What is wrong with a directory or Dockerfile path; empty is fine. */
export function pathProblem(path: string): "invalid" | undefined {
  const value = cleanPath(path)
  if (value === "") return undefined
  return value.split("/").every((s) => PATH_SEGMENT.test(s)) ? undefined : "invalid"
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

/** What is wrong with each field of a form whose name, port, host, access and user are already
 * resolved. */
export function gitProblems(
  form: GitService,
  {
    taken,
    badEnv,
    badBuildEnv,
    hasKey,
  }: { taken: readonly string[]; badEnv: boolean; badBuildEnv: boolean; hasKey: boolean },
) {
  return {
    ...builtProblems(form, { taken, badEnv, badBuildEnv }),
    url: urlProblem(form.url),
    branch: branchProblem(form.branch),
    key: form.access === "deployKey" && !hasKey,
    token: form.access === "token" && (form.token.trim() === "" || form.username.trim() === ""),
    installation: form.access === "githubApp" && !form.installationId,
  }
}

/** `<registry>/<project>/<service>`: where the Build pushes and the Deployment pulls from. */
const imageRepository = (registry: string, project: string, name: string) =>
  `${registry.replace(/\/+$/, "")}/${project}/${name}`

const credentialsName = (name: string) => `${name}-git`
/** The Secret that holds a Build's variables. */
export const buildEnvSecret = (build: string) => `${build}-build-env`

const fromSecret = (secret: string, name: string) => ({
  name,
  valueFrom: { secretKeyRef: { name: secret, key: name } },
})

/** The Build's `spec.source.git`: the repository, over SSH for a deploy key. */
function gitSource(form: GitService): Manifest {
  const repo = parseRepository(form.url)
  const directory = cleanPath(form.directory)
  const dockerfile = form.method === "dockerfile" ? cleanPath(form.dockerfile) : ""
  const url = !repo
    ? form.url
    : form.access === "deployKey"
      ? sshUrl(form.url, repo)
      : httpsUrl(repo)
  return {
    url,
    ref: form.branch.trim() || DEFAULT_BRANCH_REF,
    ...(directory ? { contextDir: directory } : {}),
    ...(dockerfile ? { dockerfile } : {}),
  }
}

/** The Build's `spec.source`: the repository, and how Zeabur Builder reads it. */
function buildSource(form: GitService, credentials: string | undefined): Manifest {
  const installationId = form.access === "githubApp" ? form.installationId : undefined
  return {
    git: gitSource(form),
    ...(credentials ? { credentialsRef: { name: credentials } } : {}),
    ...(installationId ? { githubApp: { installationId } } : {}),
  }
}

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

/**
 * The objects of a service whose image a Build makes: the Build with `source`, its build variables'
 * Secret, and the image service that runs what it builds.
 */
function builtObjects(
  form: BuiltService,
  project: string,
  registry: string,
  domains: DomainSettings,
  build: { source: Manifest; annotations?: Record<string, string> },
): Manifest[] {
  const name = form.name
  const labels = { "app.kubernetes.io/name": name }
  const repository = imageRepository(registry, project, name)
  const port = form.port.trim()
  const variables =
    port !== "" && !form.variables.some((v) => v.name === "PORT")
      ? [...form.variables, { name: "PORT", value: port }]
      : form.variables
  const service = imageObjects(
    {
      image: `${repository}:${PENDING_TAG}`,
      name,
      port,
      publicUrl: form.publicUrl,
      host: form.host,
      variables,
      secretVariables: true,
      size: form.size,
      replicas: 1,
      security: form.security,
    },
    domains,
  ).map((object) =>
    object.kind === "Deployment"
      ? {
          ...object,
          metadata: {
            ...(object.metadata as Manifest),
            annotations: { "keel.sh/policy": "force" },
          },
        }
      : object,
  )
  const objects: Manifest[] = []
  const envSecret = buildEnvSecret(name)
  if (form.buildVariables.length > 0)
    objects.push({
      apiVersion: "v1",
      kind: "Secret",
      metadata: { name: envSecret, labels },
      type: "Opaque",
      stringData: Object.fromEntries(form.buildVariables.map((v) => [v.name, v.value])),
    })
  objects.push(...service, {
    apiVersion: "builder.zeabur.com/v1alpha1",
    kind: "Build",
    metadata: { name, labels, ...(build.annotations ? { annotations: build.annotations } : {}) },
    spec: {
      builderRef: form.method,
      source: build.source,
      output: { image: repository },
      ...(form.buildVariables.length > 0
        ? { env: form.buildVariables.map((v) => fromSecret(envSecret, v.name)) }
        : {}),
      notify: [{ type: "keel", url: KEEL_WEBHOOK }],
    },
  })
  return objects
}

/** The objects the form creates, the deploy key aside: it is a change of its own. */
export function gitObjects(
  form: GitService,
  project: string,
  registry: string,
  domains: DomainSettings,
): Manifest[] {
  const name = form.name
  const labels = { "app.kubernetes.io/name": name }
  const credentials =
    form.access === "public" || form.access === "githubApp" ? undefined : credentialsName(name)
  const objects: Manifest[] = []
  if (form.access === "token")
    objects.push({
      apiVersion: "v1",
      kind: "Secret",
      metadata: { name: credentials, labels: { ...labels, [GIT_CREDENTIALS_LABEL]: "true" } },
      type: "kubernetes.io/basic-auth",
      stringData: { username: form.username.trim(), password: form.token.trim() },
    })
  objects.push(
    ...builtObjects(form, project, registry, domains, {
      source: buildSource(form, credentials),
    }),
  )
  return objects
}

export function createGitService(
  form: GitService,
  project: string,
  registry: string,
  domains: DomainSettings,
): Change[] {
  const changes: Change[] = gitObjects(form, project, registry, domains).map((object) => ({
    op: "create",
    object: object as Record<string, never>,
  }))
  if (form.access === "deployKey" && form.sealedKey)
    changes.push({ op: "deployKey", name: credentialsName(form.name), key: form.sealedKey })
  return changes
}

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
      } as unknown as Record<string, never>,
    })
  return changes
}

/** What "Deploy from Git" lacks in this cluster: components to install (`enable` entries), and
 * whether Zeabur's chart names no registry. Unknown counts as there. */
export interface GitNeeds {
  install: string[]
  registrySetting: boolean
}

export function gitNeeds({
  builds,
  capabilities,
  registry,
}: {
  builds: Schemas["ProjectBuilds"]["state"] | undefined
  capabilities: Schemas["Capability"][] | undefined
  /** `""` when Zeabur has no registry setting; `undefined` while unknown. */
  registry: string | undefined
}): GitNeeds | undefined {
  const absent = (name: string) => capabilities?.find((c) => c.name === name)?.existing === false
  const install: string[] = []
  if (builds === "not-installed") install.push("builds")
  if (absent("image-updates")) install.push("image-updates")
  const registrySetting = registry === ""
  return install.length > 0 || registrySetting ? { install, registrySetting } : undefined
}
