// A service from a container image: the objects the "Docker image" form creates, each one `create`
// change of one plan (docs/design: basic create). No `app.kubernetes.io/managed-by`: a plan
// refuses to change an object another tool manages.
import type { Schemas } from "@/api/client"

type Change = Schemas["Change"]
type Manifest = Record<string, unknown>

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
export type ServiceVariable =
  | { name: string; value: string }
  | { name: string; from: { secret: string; key: string } }

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
  volume?: { mountPath: string; size: string }
  size: ImageSize
  replicas: number
  registry?: { server: string; username: string; password: string }
}

export interface DomainSettings {
  suffix?: string | null
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
})

const NAME_MAX = 40
const DNS_LABEL = /^[a-z]([-a-z0-9]*[a-z0-9])?$/
const IMAGE = /^[a-z0-9][A-Za-z0-9._/:@-]*$/
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
      .slice(0, NAME_MAX - 3)
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
  if (name.length > NAME_MAX) return "long"
  if (!DNS_LABEL.test(name)) return "invalid"
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
  suffix && service ? `${service}-${project}.${suffix}` : ""

/** `KEY=value` lines as pasted from a `.env` file; the lines that are not, from 1. */
export function parseEnv(text: string): { variables: ServiceVariable[]; bad: number[] } {
  const variables: ServiceVariable[] = []
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

const base64 = (text: string) => btoa(String.fromCharCode(...new TextEncoder().encode(text)))

const portOf = (form: ImageService) => (form.port.trim() === "" ? undefined : Number(form.port))

/** The objects the form creates, in the order a plan creates them. */
export function imageObjects(form: ImageService, domains: DomainSettings): Manifest[] {
  const name = form.name
  const labels = { "app.kubernetes.io/name": name }
  const port = portOf(form)
  const objects: Manifest[] = []
  const plain = form.variables.filter((v) => "value" in v)
  const envSecret = form.secretVariables && plain.length > 0 ? `${name}-env` : undefined
  if (envSecret)
    objects.push({
      apiVersion: "v1",
      kind: "Secret",
      metadata: { name: envSecret, labels },
      type: "Opaque",
      stringData: Object.fromEntries(plain.map((v) => [v.name, v.value])),
    })
  const registry = form.registry
  if (registry) {
    const auth = base64(`${registry.username}:${registry.password}`)
    objects.push({
      apiVersion: "v1",
      kind: "Secret",
      metadata: { name: `${name}-registry`, labels },
      type: "kubernetes.io/dockerconfigjson",
      stringData: {
        ".dockerconfigjson": JSON.stringify({
          auths: {
            [registry.server.trim()]: {
              username: registry.username,
              password: registry.password,
              auth,
            },
          },
        }),
      },
    })
  }
  if (form.volume)
    objects.push({
      apiVersion: "v1",
      kind: "PersistentVolumeClaim",
      metadata: { name: `${name}-data`, labels },
      spec: {
        accessModes: ["ReadWriteOnce"],
        resources: { requests: { storage: form.volume.size.trim() } },
      },
    })
  const env = form.variables
    .filter((v) => !envSecret || !("value" in v))
    .map((v) =>
      "value" in v
        ? { name: v.name, value: v.value }
        : { name: v.name, valueFrom: { secretKeyRef: { name: v.from.secret, key: v.from.key } } },
    )
  const container: Manifest = {
    name,
    image: form.image.trim(),
    ...(port ? { ports: [{ containerPort: port }] } : {}),
    ...(envSecret ? { envFrom: [{ secretRef: { name: envSecret } }] } : {}),
    ...(env.length > 0 ? { env } : {}),
    resources: imageResources[form.size],
    ...(port ? { readinessProbe: { tcpSocket: { port } } } : {}),
    ...(form.volume ? { volumeMounts: [{ name: "data", mountPath: form.volume.mountPath }] } : {}),
  }
  objects.push({
    apiVersion: "apps/v1",
    kind: "Deployment",
    metadata: { name, labels },
    spec: {
      replicas: form.volume ? 1 : form.replicas,
      // A ReadWriteOnce claim is mounted by one node at a time: the old pod stops first.
      ...(form.volume ? { strategy: { type: "Recreate" } } : {}),
      selector: { matchLabels: labels },
      template: {
        metadata: { labels },
        spec: {
          ...(registry ? { imagePullSecrets: [{ name: `${name}-registry` }] } : {}),
          containers: [container],
          ...(form.volume
            ? { volumes: [{ name: "data", persistentVolumeClaim: { claimName: `${name}-data` } }] }
            : {}),
        },
      },
    },
  })
  if (port) {
    objects.push({
      apiVersion: "v1",
      kind: "Service",
      metadata: { name, labels },
      spec: { selector: labels, ports: [{ name: "http", port, targetPort: port }] },
    })
    if (form.publicUrl) {
      const host = form.host.trim()
      const issuer = domains.clusterIssuer ?? undefined
      objects.push({
        apiVersion: "networking.k8s.io/v1",
        kind: "Ingress",
        metadata: {
          name,
          labels,
          ...(issuer ? { annotations: { "cert-manager.io/cluster-issuer": issuer } } : {}),
        },
        spec: {
          ...(issuer ? { tls: [{ hosts: [host], secretName: `${name}-tls` }] } : {}),
          rules: [
            {
              host,
              http: {
                paths: [
                  {
                    path: "/",
                    pathType: "Prefix",
                    backend: { service: { name, port: { number: port } } },
                  },
                ],
              },
            },
          ],
        },
      })
    }
  }
  return objects
}

export const createImageService = (form: ImageService, domains: DomainSettings): Change[] =>
  imageObjects(form, domains).map((object) => ({
    op: "create",
    object: object as Record<string, never>,
  }))

/** What a person may create, by kind, as the creatable list hints; absent kinds are unknown. */
export type Rights = (group: string, resource: string) => Schemas["ObjectAccess"]

export function rightsOf(list: Schemas["CreatableList"] | undefined): Rights {
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
