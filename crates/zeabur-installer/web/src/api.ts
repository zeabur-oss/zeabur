// The installer's API (crates/zeabur-installer/src/gui.rs). Every call carries the session cookie
// the printed link set; POST bodies are JSON.

export type Status = "pass" | "warn" | "fail"

export interface Check {
  name: string
  status: Status
  detail: string
  code: string
  values: string[]
}

export interface Input {
  name: string
  description: string
  secret: boolean
  generated: boolean
  required: boolean
  /** The installer fills it, so the page does not ask. */
  filled: boolean
  default?: unknown
}

export interface CatalogComponent {
  id: string
  capability: string
  displayName: string
  default: "recommended" | "optional" | "unsupported"
  /** Offered in a personal install. */
  personal: boolean
  /** A personal install offers it only on a server with this much memory. */
  personalMinMemoryMib?: number | null
  /** Zeabur needs it to run, so it is never suggested for leaving out. */
  required: boolean
  /** Where it runs; empty when everywhere. */
  distributions: string[]
  /** The components installed before it because it needs them. */
  needs?: string[]
  /** Measured idle memory on this server's architecture. */
  idleMemoryMib: number | null
  security: string
  privileged: boolean
  inputs: Input[]
}

/** The Kubernetes of a team install. */
export type Distribution = "k3s" | "rke2"

export const distributionTitle: Record<Distribution, string> = { k3s: "K3s", rke2: "RKE2" }

/** Whether a component runs on the distribution: an empty list means everywhere. */
export const runsOn = (component: CatalogComponent, distribution: string) =>
  component.distributions.length === 0 || component.distributions.includes(distribution)
export type AccessMode = "domain" | "local"

export interface Config {
  enable: string[]
  disable: string[]
  profile: string
  access: { mode: AccessMode; identity: string; zeabur: string; suffixes?: string[] }
  zeabur: boolean
  inputs: Record<string, Record<string, unknown>>
  secretFiles: Record<string, Record<string, string>>
  /** Components apply may remove although that deletes the data they keep. */
  allowDataLoss?: string[]
  /** The experience program; on unless `false`. */
  telemetry?: boolean
  /** The Zeabur panel's update check; on unless `false`. */
  updateCheck?: boolean
  distribution: Distribution
  k3sVersion?: string | null
  rke2Version?: string | null
  host: { tlsSan: string[]; requireEncryptedDisk: boolean; nodeName?: string }
  sources: Record<string, unknown>
}

/** Who the install is for: one person on KubeSolo, or a team on K3s or RKE2 with the Zeabur
 * panel. */
export type Use = "personal" | "team"

export interface PersonalConfig {
  use: "personal"
  enable: string[]
  disable: string[]
  inputs: Record<string, Record<string, unknown>>
  secretFiles: Record<string, Record<string, string>>
  allowDataLoss?: string[]
  telemetry?: boolean
  host: { tlsSan: string[]; requireEncryptedDisk: boolean; nodeName?: string }
  sources: Record<string, unknown>
}

export interface ServerState {
  facts: {
    arch: string
    hostname: string
    memoryMib: number
    /** The name an earlier install gave this node; it cannot change. */
    nodeName: string | null
  } | null
  /** The checks for a team install on each distribution. */
  checks: Record<Distribution, Check[]>
  /** The checks for a personal install. */
  personalChecks: Check[]
  addresses: string[]
  /** What the saved configuration is for. */
  use: Use
  config: Config | null
  personal?: PersonalConfig
  /** Below this, a team install on the distribution does not fit. */
  teamMinMemoryMib?: Record<Distribution, number>
  /** From this much memory, RKE2 is the suggested distribution. */
  rke2SuggestedMib?: number
  configExists: boolean
  configPath: string
  catalog: { components: CatalogComponent[] } | null
  catalogError: string | null
  applying: boolean
}

export interface Warning {
  code: string
  values: string[]
  text: string
}

export type Release =
  | { kind: "helmChart"; name: string; object: { spec?: { chart?: string; version?: string } } }
  | { kind: "manifests" | "urls"; name: string }

export interface PlannedComponent {
  id: string
  displayName: string
  why: "default" | "enabled" | "dependency"
  idleMemoryMib: number | null
  releases: Release[]
}

/** An installed component the configuration no longer has; apply deletes its HelmCharts. */
export interface Removal {
  id: string
  displayName: string
  helmCharts: string[]
  /** Namespaces that stay, with what is left in them. */
  namespaces: string[]
  /** What stays after `helm uninstall`; no CRDs when the release owns them. */
  leftovers: string[]
  /** CRDs that `helm uninstall` deletes, with all their objects. */
  deletedCrds: string[]
  /** Manifest releases, whose objects stay. */
  manifests: string[]
  /** What still uses it; apply stops while there is any. */
  usedBy: string[]
  losesData: boolean
  /** `allowDataLoss` names it. */
  confirmed: boolean
}

/** What apply changes against the install record of an earlier apply. */
export interface Changes {
  /** The installer version that wrote the record. */
  installer: string
  added: string[]
  removed: string[]
  /** Left out, but with no chart to uninstall: what they installed keeps running. */
  kept: string[]
  /** Recorded components the catalog renamed; the new one takes over the old one's release. */
  replaced: { from: string; to: string }[]
  charts: { component: string; release: string; from: string; to: string; values: boolean }[]
  /** New defaults of this catalog that apply leaves out unless chosen. */
  offered: string[]
  version: { from: string; to: string } | null
  hostFiles: string[]
  /** Apply restarts K3s, RKE2 or KubeSolo. */
  restart: boolean
}

interface PlanBase {
  components: PlannedComponent[]
  /** Null on a first install. */
  changes: Changes | null
  leftOut: Record<string, string>
  removals: Removal[]
  /** The distribution itself (K3s, RKE2 or KubeSolo), then the measured components. */
  memory: { clusterMib: number; componentsMib: number; unmeasured: number; hostMib: number }
  warnings: Warning[]
}

export interface TeamPlan extends PlanBase {
  use?: undefined
  cluster: { distribution: Distribution; version: string; pinned: boolean }
  access: AccessMode
  identity: { issuer: string; host: string } | null
  zeabur: { url: string; values?: { domains?: { suffixes?: string[] } } } | null
}

export interface PersonalPlan extends PlanBase {
  use: "personal"
  facts: { hostname: string; nodeIps: string[] }
  kubesolo: { version: string; kubernetes: string }
  helmController: { version: string }
}

export type Plan = TeamPlan | PersonalPlan

export type Planned = { plan: Plan; text: string; yaml: string } | { error: string }

export interface Progress {
  lines: string[]
  next: number
  running: boolean
  code: number | null
}

export type Secrets = Record<string, Record<string, string>>

async function call<T>(path: string, body?: unknown): Promise<T> {
  const response = await fetch(
    path,
    body === undefined
      ? {}
      : {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(body),
        },
  )
  if (!response.ok) throw new Error((await response.text()) || `HTTP ${response.status}`)
  return (await response.json()) as T
}

export const api = {
  state: () => call<ServerState>("/api/state"),
  resolve: (name: string) =>
    call<{ name: string; addresses: string[] }>(`/api/resolve?name=${encodeURIComponent(name)}`),
  plan: (config: Config | PersonalConfig, secrets: Secrets) =>
    call<Planned>("/api/plan", { config, secrets }),
  apply: (config: Config | PersonalConfig, secrets: Secrets) =>
    call<{ started: true } | { error: string }>("/api/apply", { config, secrets }),
  progress: (from: number) => call<Progress>(`/api/apply?from=${from}`),
}
