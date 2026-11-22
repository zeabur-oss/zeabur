// A service from a container image: the "Docker image" form, sent as one `createImageService`
// change. The server makes its objects.
import type { Schemas } from "@/api/client"
import { rules } from "@/api/rules"
import { dashedSuffix } from "@/model/domains"
import { type Profile } from "@/model/pod-security"
import { textProblem } from "@/model/rules"

type Change = Schemas.Change

export type ImageSize = "small" | "medium" | "large"
export const imageSizes: ImageSize[] = ["small", "medium", "large"]

export const imageResources: Record<
  ImageSize,
  { requests: { cpu: string; memory: string }; limits: { memory: string } }
> = {
  small: { requests: { cpu: "100m", memory: "128Mi" }, limits: { memory: "256Mi" } },
  medium: { requests: { cpu: "250m", memory: "256Mi" }, limits: { memory: "512Mi" } },
  large: { requests: { cpu: "500m", memory: "512Mi" }, limits: { memory: "1Gi" } },
}

/** A variable of the new service: a value, or a key of a Secret (a database's connection). */
export type ServiceVariable = Schemas.ServiceVariable

export type ServiceVolume = Schemas.ServiceVolume

export interface ImageService {
  image: string
  name: string
  /** Empty: a worker, which takes no connections. */
  port: string
  publicUrl: boolean
  host: string
  variables: ServiceVariable[]
  /** Plain values go in the Secret `<name>-env` rather than in the Deployment. */
  secretVariables: boolean
  volumes?: ServiceVolume[]
  size: ImageSize
  replicas: number
  registry?: { server: string; username: string; password: string }
  security: Profile
  /** The container's command and arguments, when not the image's. */
  command?: string[]
  args?: string[]
  /** Ports besides `port` the Kubernetes Service carries. */
  morePorts?: number[]
  /** Hosts and ports an init container waits for before the app starts, with its image. */
  waitFor?: { hosts: { host: string; port: number }[]; image: string }
}

export interface DomainSettings {
  suffixes?: string[]
  clusterIssuer?: string | null
}

export const newImageService = (): ImageService => ({
  image: "",
  name: "",
  port: "",
  publicUrl: false,
  host: "",
  variables: [],
  secretVariables: true,
  size: "small",
  replicas: 1,
  security: "baseline",
})

const NAME = rules["ImageServiceChange.name"]
const IMAGE = new RegExp(rules["ImageServiceChange.image"].pattern)
const HOST = /^(?=.{1,253}$)([a-z0-9]([-a-z0-9]{0,61}[a-z0-9])?\.)+[a-z]([-a-z0-9]{0,61}[a-z0-9])?$/
const VARIABLE = /^[A-Za-z_][A-Za-z0-9_]*$/

/** The repository's last path segment, without tag or digest. */
function repository(image: string): string {
  const withoutDigest = image.split("@")[0] ?? ""
  const last = withoutDigest.split("/").at(-1) ?? ""
  return last.split(":")[0] ?? ""
}

/** Whether the reference pins a tag or a digest; without one the registry serves `latest`. */
export function hasTag(image: string): boolean {
  const value = image.trim()
  return value.includes("@") || (value.split("/").at(-1) ?? "").includes(":")
}

/** A name for the service from its image, as a DNS label, with `-2`, `-3`… past those taken. */
export function nameFromImage(image: string, taken: readonly string[]): string {
  const base =
    repository(image.trim().toLowerCase())
      .replace(/[^a-z0-9-]+/g, "-")
      .replace(/^[^a-z]+/, "")
      .slice(0, NAME.maxLength - 3)
      .replace(/-+$/, "") || "app"
  if (!taken.includes(base)) return base
  for (let n = 2; ; n += 1) if (!taken.includes(`${base}-${n}`)) return `${base}-${n}`
}

export type FieldProblem = "empty" | "invalid" | "long" | "taken"

export function imageProblem(image: string): "empty" | "invalid" | undefined {
  const value = image.trim()
  if (value === "") return "empty"
  return IMAGE.test(value) ? undefined : "invalid"
}

export function serviceNameProblem(
  name: string,
  taken: readonly string[],
): FieldProblem | undefined {
  if (name === "") return "empty"
  const problem = textProblem(name, NAME)
  if (problem) return problem
  return taken.includes(name) ? "taken" : undefined
}

export function portProblem(port: string): "invalid" | undefined {
  if (port.trim() === "") return undefined
  const n = Number(port.trim())
  return Number.isInteger(n) && n >= 1 && n <= 65535 ? undefined : "invalid"
}

export function hostProblem(host: string): "empty" | "invalid" | undefined {
  const value = host.trim()
  if (value === "") return "empty"
  return HOST.test(value) ? undefined : "invalid"
}

/** `<service>-<project>.<suffix>`, or nothing to suggest without a suffix. */
export const suggestedHost = (service: string, project: string, suffix?: string | null) =>
  suffix && service ? `${service}-${project}.${dashedSuffix(suffix)}` : ""

/** The name a built service gets before the person types one: from the repository or folder,
 * else `web`. */
export const builtName = (source: string, taken: readonly string[]) =>
  nameFromImage(source || "web", taken)

/** What the host field shows while it is empty: the shape `suggestedHost` gives. */
export const hostPlaceholder = (project: string, suffix?: string | null) =>
  suffix ? `<name>-${project}.${dashedSuffix(suffix)}` : "web.example.com"

/** `KEY=value` lines as pasted from a `.env` file; the lines that are not, from 1. */
export function parseEnv(text: string): { variables: Schemas.NameValue[]; bad: number[] } {
  const variables: Schemas.NameValue[] = []
  const bad: number[] = []
  text.split("\n").forEach((raw, i) => {
    const line = raw.trim()
    if (line === "" || line.startsWith("#")) return
    const match = /^(?:export\s+)?([^=\s]+)\s*=\s*(.*)$/.exec(line)
    const name = match?.[1] ?? ""
    if (!match || !VARIABLE.test(name)) {
      bad.push(i + 1)
      return
    }
    let value = match[2] ?? ""
    const quote = value[0]
    if ((quote === '"' || quote === "'") && value.length > 1 && value.endsWith(quote))
      value = value.slice(1, -1)
    const at = variables.findIndex((v) => v.name === name)
    if (at >= 0) variables.splice(at, 1)
    variables.push({ name, value })
  })
  return { variables, bad }
}

/** The change the form sends. */
export function imageChange(form: ImageService): Schemas.ImageServiceChange {
  const port = form.port.trim() === "" ? undefined : Number(form.port)
  return {
    name: form.name,
    image: form.image.trim(),
    ...(port ? { port, morePorts: form.morePorts ?? [] } : {}),
    ...(port && form.publicUrl ? { host: form.host.trim() } : {}),
    variables: form.variables,
    secretVariables: form.secretVariables,
    volumes: form.volumes ?? [],
    size: form.size,
    replicas: form.replicas,
    ...(form.registry ? { registry: form.registry } : {}),
    security: form.security,
    command: form.command ?? [],
    args: form.args ?? [],
    ...(form.waitFor ? { waitFor: form.waitFor } : {}),
  }
}

export const createImageService = (form: ImageService): Change[] => [
  { op: "createImageService", ...imageChange(form) },
]

/** What a person may create, by kind, as the creatable list hints; absent kinds are unknown. */
export type Rights = (group: string, resource: string) => Schemas.ObjectAccess

export function rightsOf(list: Schemas.CreatableList | undefined): Rights {
  return (group, resource) =>
    list?.items.find((k) => k.group === group && k.resource === resource)?.access ?? "unknown"
}

/** Which parts of the form the person's rights allow; `denied` hides, `unknown` offers. */
export function imageOptions(rights: Rights) {
  const may = (group: string, resource: string) => rights(group, resource) !== "denied"
  return {
    deploy: may("apps", "deployments"),
    port: may("", "services"),
    publicUrl: may("", "services") && may("networking.k8s.io", "ingresses"),
    secrets: may("", "secrets"),
    volume: may("", "persistentvolumeclaims"),
  }
}
