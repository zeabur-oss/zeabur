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
  default?: unknown
}

export interface CatalogComponent {
  id: string
  capability: string
  displayName: string
  default: "recommended" | "optional" | "unsupported"
  supported: boolean
  /** Offered in a personal install; older installers leave it out. */
  personal?: boolean
  /** A personal install offers it only on a server with this much memory. */
  personalMinMemoryMib?: number | null
  /** Where it runs, when not everywhere; older installers leave it out. */
  distributions?: string[]
  /** The components installed before it because it needs them. */
  needs?: string[]
  /** Measured idle memory on this server's architecture; older installers leave it out. */
  idleMemoryMib?: number | null
  security: string
  privileged: boolean
  inputs: Input[]
}

export type Preset = "minimal" | "standard" | "secure"
export type AccessMode = "domain" | "local"

export interface Config {
  preset: Preset
  enable: string[]
  disable: string[]
  profile: string
  access: { mode: AccessMode; identity: string; zeabur: string; suffixes?: string[] }
  zeabur: boolean
  inputs: Record<string, Record<string, unknown>>
  secretFiles: Record<string, Record<string, string>>
  rke2Version?: string | null
  host: { tlsSan: string[]; requireEncryptedDisk: boolean }
  sources: Record<string, unknown>
}

/** Who the install is for: one person on KubeSolo, or a team on RKE2 with the Zeabur panel. */
export type Use = "personal" | "team"

export interface PersonalConfig {
  use: "personal"
  enable: string[]
  disable: string[]
  inputs: Record<string, Record<string, unknown>>
  secretFiles: Record<string, Record<string, string>>
  host: { tlsSan: string[]; requireEncryptedDisk: boolean }
  sources: Record<string, unknown>
}

export interface ServerState {
  facts: { arch: string; hostname: string; memoryMib: number } | null
  checks: Check[]
  /** The checks for a personal install; older installers leave them out. */
  personalChecks?: Check[]
  addresses: string[]
  /** What the saved configuration is for; older installers leave it out. */
  use?: Use
  config: Config | null
  personal?: PersonalConfig
  /** Below this, only a personal install fits. */
  teamMinMemoryMib?: number
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
  why: "preset" | "enabled" | "dependency"
  idleMemoryMib: number | null
  releases: Release[]
}

interface PlanBase {
  components: PlannedComponent[]
  leftOut: Record<string, string>
  /** The distribution itself (RKE2 or KubeSolo), then the measured components. */
  memory: { clusterMib: number; componentsMib: number; unmeasured: number; hostMib: number }
  warnings: Warning[]
}

export interface TeamPlan extends PlanBase {
  use?: undefined
  rke2: { version: string; pinned: boolean }
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
