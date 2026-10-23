// A service from a Git repository: Zeabur Builder's Build, and the objects of an image service
// whose image is the Build's (docs/design: basic create). The Deployment names the Build's
// repository with a tag no build makes, and Keel moves it to each image the Build pushes.
import type { Schemas } from "@/api/client"
import { KEEL_WEBHOOK } from "@/model/builds"
import { imageObjects, type DomainSettings, type ImageService } from "@/model/image-service"

type Change = Schemas["Change"]
type Manifest = Record<string, unknown>

export type BuildMethod = "railpack" | "dockerfile"
export const buildMethods: BuildMethod[] = ["railpack", "dockerfile"]

export type Access = "public" | "deployKey" | "token"

/** The ref Zeabur Builder takes for the repository's default branch. */
export const DEFAULT_BRANCH_REF = "HEAD"

/** The tag the Deployment names until the first build. */
const PENDING_TAG = "pending"
const GIT_CREDENTIALS_LABEL = "builder.zeabur.com/git-credentials"
/** Hosts whose SSH host keys Zeabur knows, so a deploy key works there. */
const DEPLOY_KEY_HOSTS = ["github.com", "gitlab.com"] as const

export interface Repository {
  host: string
  /** `owner/repo`, or deeper for GitLab groups; no `.git`. */
  path: string
}

export interface GitService extends Pick<
  ImageService,
  "name" | "port" | "publicUrl" | "host" | "variables" | "size"
> {
  url: string
  /** Empty: the repository's default branch. */
  branch: string
  /** Empty: the repository's root. */
  directory: string
  method: BuildMethod
  access: Access
  /** For `token`. */
  username: string
  token: string
  /** For `deployKey`: the sealed key `POST /deploy-keys` answered. */
  sealedKey?: string
}

export const newGitService = (): GitService => ({
  url: "",
  branch: "",
  directory: "",
  method: "railpack",
  access: "public",
  username: "",
  token: "",
  name: "",
  port: "",
  publicUrl: false,
  host: "",
  variables: [],
  size: "small",
})

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
const sshUrl = (repo: Repository) => `git@${repo.host}:${repo.path}.git`

export const deployKeyHost = (repo: Repository | undefined) =>
  DEPLOY_KEY_HOSTS.find((h) => h === repo?.host)

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

export function directoryProblem(directory: string): "invalid" | undefined {
  const value = directory
    .trim()
    .replace(/^\.?\/+/, "")
    .replace(/\/+$/, "")
  if (value === "") return undefined
  return value.split("/").every((s) => s !== "" && s !== ".." && SEGMENT.test(s))
    ? undefined
    : "invalid"
}

/** `<registry>/<project>/<service>`: where the Build pushes and the Deployment pulls from. */
const imageRepository = (registry: string, project: string, name: string) =>
  `${registry.replace(/\/+$/, "")}/${project}/${name}`

const credentialsName = (name: string) => `${name}-git`

/** The objects the form creates, the deploy key aside: it is a change of its own. */
export function gitObjects(
  form: GitService,
  project: string,
  registry: string,
  domains: DomainSettings,
): Manifest[] {
  const repo = parseRepository(form.url)
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
  const credentials = form.access === "public" ? undefined : credentialsName(name)
  const objects: Manifest[] = []
  if (form.access === "token")
    objects.push({
      apiVersion: "v1",
      kind: "Secret",
      metadata: { name: credentials, labels: { ...labels, [GIT_CREDENTIALS_LABEL]: "true" } },
      type: "kubernetes.io/basic-auth",
      stringData: { username: form.username.trim(), password: form.token.trim() },
    })
  const directory = form.directory
    .trim()
    .replace(/^\.?\/+/, "")
    .replace(/\/+$/, "")
  objects.push(...service, {
    apiVersion: "builder.zeabur.com/v1alpha1",
    kind: "Build",
    metadata: { name, labels },
    spec: {
      builderRef: form.method,
      source: {
        git: {
          url: repo ? (form.access === "deployKey" ? sshUrl(repo) : httpsUrl(repo)) : form.url,
          ref: form.branch.trim() || DEFAULT_BRANCH_REF,
          ...(directory ? { contextDir: directory } : {}),
        },
        ...(credentials ? { credentialsRef: { name: credentials } } : {}),
      },
      output: { image: repository },
      notify: [{ type: "keel", url: KEEL_WEBHOOK }],
    },
  })
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

/** Why "Deploy from Git" cannot be used in this cluster, when it cannot. Unknown stays open. */
export type GitMissing = "builder" | "registry" | "keel" | "registrySetting"

export function gitMissing({
  builds,
  capabilities,
  registry,
}: {
  builds: Schemas["ProjectBuilds"]["state"] | undefined
  capabilities: Schemas["Capability"][] | undefined
  /** `""` when Zeabur has no registry setting; `undefined` while unknown. */
  registry: string | undefined
}): GitMissing | undefined {
  if (builds === "not-installed") return "builder"
  const absent = (name: string) => capabilities?.find((c) => c.name === name)?.existing === false
  if (absent("container-registry")) return "registry"
  if (absent("image-updates")) return "keel"
  if (registry === "") return "registrySetting"
  return undefined
}
