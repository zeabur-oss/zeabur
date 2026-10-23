// A service from a container image: the objects the "Docker image" form creates, each one `create`
// change of one plan (docs/design: basic create). No `app.kubernetes.io/managed-by`: a plan
// refuses to change an object another tool manages.
import type { Schemas } from "@/api/client"
import { lowersProject, templateSecurity, type Profile } from "@/model/pod-security"

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
  | { name: string; value: string; expands?: boolean }
  | { name: string; from: { secret: string; key: string } }

/** A claim `<service>-<name>` mounted at `mountPath`. */
export interface ServiceVolume {
  name: string
  mountPath: string
  size: string
  readOnly?: boolean
}

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
  security: "restricted",
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

type Labels = Record<string, string>

const secretable = (v: ServiceVariable): v is Extract<ServiceVariable, { value: string }> =>
  "value" in v && !v.expands

/** Plain values go in `<name>-env` when the form asks; a value that reads another variable with
 * $(…) stays in `env`, as Kubernetes expands it only there. */
function envOf(form: ImageService) {
  const plain = form.variables.filter(secretable)
  const envSecret = form.secretVariables && plain.length > 0 ? `${form.name}-env` : undefined
  const env = form.variables
    .filter((v) => !envSecret || !secretable(v))
    .map((v) =>
      "value" in v
        ? { name: v.name, value: v.value }
        : { name: v.name, valueFrom: { secretKeyRef: { name: v.from.secret, key: v.from.key } } },
    )
  return { plain, envSecret, env }
}

function registrySecret(form: ImageService, labels: Labels): Manifest | undefined {
  const registry = form.registry
  if (!registry) return undefined
  const auth = base64(`${registry.username}:${registry.password}`)
  return {
    apiVersion: "v1",
    kind: "Secret",
    metadata: { name: `${form.name}-registry`, labels },
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
  }
}

const claimOf = (form: ImageService, volume: ServiceVolume, labels: Labels): Manifest => ({
  apiVersion: "v1",
  kind: "PersistentVolumeClaim",
  metadata: { name: `${form.name}-${volume.name}`, labels },
  spec: {
    accessModes: ["ReadWriteOnce"],
    resources: { requests: { storage: volume.size.trim() } },
  },
})

const optional = (on: unknown, value: Manifest): Manifest => (on ? value : {})

function containerOf(form: ImageService, ports: number[], env: ReturnType<typeof envOf>) {
  const volumes = form.volumes ?? []
  const port = ports[0]
  return {
    name: form.name,
    image: form.image.trim(),
    ...optional(form.command?.length, { command: form.command }),
    ...optional(form.args?.length, { args: form.args }),
    ...optional(port, { ports: ports.map((p) => ({ containerPort: p })) }),
    ...optional(env.envSecret, { envFrom: [{ secretRef: { name: env.envSecret } }] }),
    ...optional(env.env.length > 0, { env: env.env }),
    resources: imageResources[form.size],
    ...optional(port, { readinessProbe: { tcpSocket: { port } } }),
    ...optional(volumes.length > 0, {
      volumeMounts: volumes.map((v) => ({
        name: v.name,
        mountPath: v.mountPath,
        ...optional(v.readOnly, { readOnly: true }),
      })),
    }),
    ...templateSecurity(form.security).container,
  }
}

/** Init containers that wait until each host accepts connections on its port. */
const waitContainers = (wait: NonNullable<ImageService["waitFor"]>) =>
  wait.hosts.map(({ host, port }, i) => ({
    name: `wait-${i + 1}`,
    image: wait.image,
    command: ["sh", "-c", `until nc -z -w 2 ${host} ${port}; do sleep 2; done`],
    resources: { requests: { cpu: "10m", memory: "16Mi" }, limits: { memory: "32Mi" } },
    // As nobody, so a pod that must not run as root starts it too.
    securityContext: {
      allowPrivilegeEscalation: false,
      capabilities: { drop: ["ALL"] },
      runAsNonRoot: true,
      runAsUser: 65534,
    },
  }))

function deploymentOf(form: ImageService, container: Manifest, labels: Labels): Manifest {
  const volumes = form.volumes ?? []
  const stateful = volumes.length > 0
  return {
    apiVersion: "apps/v1",
    kind: "Deployment",
    metadata: { name: form.name, labels },
    spec: {
      replicas: stateful ? 1 : form.replicas,
      // A ReadWriteOnce claim is mounted by one node at a time: the old pod stops first.
      ...optional(stateful, { strategy: { type: "Recreate" } }),
      selector: { matchLabels: labels },
      template: {
        metadata: { labels },
        spec: {
          ...optional(form.registry, { imagePullSecrets: [{ name: `${form.name}-registry` }] }),
          ...templateSecurity(form.security).pod,
          ...optional(form.waitFor?.hosts.length, {
            initContainers: form.waitFor && waitContainers(form.waitFor),
          }),
          containers: [container],
          ...optional(stateful, {
            volumes: volumes.map((v) => ({
              name: v.name,
              persistentVolumeClaim: { claimName: `${form.name}-${v.name}` },
            })),
          }),
        },
      },
    },
  }
}

const serviceOf = (name: string, ports: number[], labels: Labels): Manifest => ({
  apiVersion: "v1",
  kind: "Service",
  metadata: { name, labels },
  spec: {
    selector: labels,
    ports: ports.map((p, i) => ({ name: i === 0 ? "http" : `port-${p}`, port: p, targetPort: p })),
  },
})

function ingressOf(form: ImageService, port: number, domains: DomainSettings, labels: Labels) {
  const host = form.host.trim()
  const issuer = domains.clusterIssuer ?? undefined
  return {
    apiVersion: "networking.k8s.io/v1",
    kind: "Ingress",
    metadata: {
      name: form.name,
      labels,
      ...optional(issuer, { annotations: { "cert-manager.io/cluster-issuer": issuer } }),
    },
    spec: {
      ...optional(issuer, { tls: [{ hosts: [host], secretName: `${form.name}-tls` }] }),
      rules: [
        {
          host,
          http: {
            paths: [
              {
                path: "/",
                pathType: "Prefix",
                backend: { service: { name: form.name, port: { number: port } } },
              },
            ],
          },
        },
      ],
    },
  }
}

/** The objects the form creates, in the order a plan creates them. */
export function imageObjects(form: ImageService, domains: DomainSettings): Manifest[] {
  const labels = { "app.kubernetes.io/name": form.name }
  const port = portOf(form)
  const ports = port ? [port, ...(form.morePorts ?? [])] : []
  const env = envOf(form)
  const envSecret: Manifest[] = env.envSecret
    ? [
        {
          apiVersion: "v1",
          kind: "Secret",
          metadata: { name: env.envSecret, labels },
          type: "Opaque",
          stringData: Object.fromEntries(env.plain.map((v) => [v.name, v.value])),
        },
      ]
    : []
  const service = port ? [serviceOf(form.name, ports, labels)] : []
  const ingress = port && form.publicUrl ? [ingressOf(form, port, domains, labels)] : []
  const registry = registrySecret(form, labels)
  return [
    ...envSecret,
    ...(registry ? [registry] : []),
    ...(form.volumes ?? []).map((v) => claimOf(form, v, labels)),
    deploymentOf(form, containerOf(form, ports, env), labels),
    ...service,
    ...ingress,
  ]
}

/** `enforced`: the project's Pod Security level, which Compatible lowers in the same plan. */
export const createImageService = (
  form: ImageService,
  domains: DomainSettings,
  enforced?: Schemas["PodSecurityLevel"] | null,
): Change[] => [
  ...imageObjects(form, domains).map((object): Change => ({
    op: "create",
    object: object as Record<string, never>,
  })),
  ...projectLevel(form.security, enforced),
]

/** The project's level a new service of `profile` needs, as a change when it must be lowered. */
export const projectLevel = (
  profile: Profile,
  enforced: Schemas["PodSecurityLevel"] | null | undefined,
): Change[] => (lowersProject(profile, enforced) ? [{ op: "podSecurity", level: "baseline" }] : [])

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
