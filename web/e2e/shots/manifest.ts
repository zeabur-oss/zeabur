// The screenshot manifest (docs/shots.yaml), as JSON from `yq -o=json`. Run by
// scripts/docs-shots.sh: `bun e2e/shots/manifest.ts <shots.json> [name]` checks it and prints each
// distinct demo server (its app and environment) as one line of JSON.
import { readFileSync } from "node:fs"

export type Step =
  | { click: string }
  | { fill: string; value: string }
  | { hover: string }
  | { press: string }
  | { wait: string }

export const apps = ["zeabur", "installer"] as const
export type App = (typeof apps)[number]

export type Shot = {
  id: string
  app: App
  path: string
  wait?: string
  steps: Step[]
  clip?: string
  viewport: "desktop" | "phone"
  env: Record<string, string>
  locales: Locale[]
  themes: Theme[]
}

export const locales = { en: "en-US", "zh-TW": "zh-TW", "zh-CN": "zh-CN" }
export const themes = ["light", "dark"] as const
export type Locale = keyof typeof locales
export type Theme = (typeof themes)[number]

export const viewports = {
  desktop: { width: 1440, height: 900 },
  phone: { width: 390, height: 844 },
}

const shotKeys = [
  "id",
  "app",
  "path",
  "wait",
  "steps",
  "clip",
  "viewport",
  "env",
  "locales",
  "themes",
]
const stepKeys: Record<string, string[]> = {
  click: ["click"],
  fill: ["fill", "value"],
  hover: ["hover"],
  press: ["press"],
  wait: ["wait"],
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value)
}

function step(value: unknown, at: string): Step {
  if (!isRecord(value)) throw new Error(`${at}: expected a mapping`)
  const kind = Object.keys(stepKeys).find((key) => key in value)
  if (!kind) throw new Error(`${at}: expected one of ${Object.keys(stepKeys).join(", ")}`)
  for (const key of Object.keys(value)) {
    if (!stepKeys[kind].includes(key)) throw new Error(`${at}: unknown key "${key}"`)
  }
  for (const key of stepKeys[kind]) {
    if (typeof value[key] !== "string" || value[key] === "")
      throw new Error(`${at}.${key}: expected a string`)
  }
  return value as Step
}

function check(ok: boolean, message: string) {
  if (!ok) throw new Error(message)
}

function isSelector(value: unknown): boolean {
  return value === undefined || (typeof value === "string" && value !== "")
}

function environment(value: unknown, at: string): Record<string, string> {
  const env = value ?? {}
  if (!isRecord(env)) throw new Error(`${at}: env must be a mapping`)
  for (const [key, setting] of Object.entries(env)) {
    check(/^ZEABUR_DEMO_[A-Z0-9_]+$/.test(key), `${at}: env ${key} is not a ZEABUR_DEMO_ variable`)
    check(typeof setting === "string", `${at}: env ${key} must be a string`)
  }
  return env as Record<string, string>
}

function subset<T extends string>(value: unknown, allowed: readonly T[], at: string): T[] {
  check(
    Array.isArray(value) && value.length > 0 && value.every((item) => allowed.includes(item as T)),
    `${at} must be a list of ${allowed.join(", ")}`,
  )
  return value as T[]
}

function shot(value: unknown, index: number): Shot {
  if (!isRecord(value)) throw new Error(`shot ${index}: expected a mapping`)
  const at = typeof value.id === "string" ? value.id : `shot ${index}`
  for (const key of Object.keys(value)) check(shotKeys.includes(key), `${at}: unknown key "${key}"`)
  const { id, app = "zeabur", path, wait, clip, viewport = "desktop", steps = [] } = value
  const { locales: shotLocales = Object.keys(locales), themes: shotThemes = ["light"] } = value
  check(
    typeof id === "string" && /^[a-z0-9]+(-[a-z0-9]+)*$/.test(id),
    `${at}: id must be kebab-case`,
  )
  check(apps.includes(app as App), `${at}: app must be ${apps.join(" or ")}`)
  check(typeof path === "string" && path.startsWith("/"), `${at}: path must start with /`)
  check(isSelector(wait), `${at}: wait must be a CSS selector`)
  check(isSelector(clip), `${at}: clip must be a CSS selector`)
  check(viewport === "desktop" || viewport === "phone", `${at}: viewport must be desktop or phone`)
  if (!Array.isArray(steps)) throw new Error(`${at}: steps must be a list`)
  return {
    id: id as string,
    app: app as App,
    path: path as string,
    wait: wait as string | undefined,
    steps: steps.map((s, i) => step(s, `${at}: step ${i + 1}`)),
    clip: clip as string | undefined,
    viewport: viewport as Shot["viewport"],
    env: environment(value.env, at),
    locales: subset(shotLocales, Object.keys(locales) as Locale[], `${at}: locales`),
    themes: subset(shotThemes, themes, `${at}: themes`),
  }
}

/** The demo server a shot needs as one canonical line, for grouping shots by server. */
export function serverKey({ app, env }: Pick<Shot, "app" | "env">): string {
  return JSON.stringify({ app, env: Object.fromEntries(Object.entries(env).sort()) })
}

export function readManifest(file: string): Shot[] {
  const data: unknown = JSON.parse(readFileSync(file, "utf8"))
  if (!Array.isArray(data)) throw new Error("expected a list of shots")
  const shots = data.map(shot)
  const seen = new Set<string>()
  for (const { id } of shots) {
    if (seen.has(id)) throw new Error(`${id}: duplicate id`)
    seen.add(id)
  }
  return shots
}

if (import.meta.main) {
  try {
    const shots = readManifest(process.argv[2])
    for (const key of new Set(shots.map(serverKey))) console.log(key)
  } catch (error) {
    console.error(`${process.argv[3] ?? process.argv[2]}: ${(error as Error).message}`)
    process.exit(1)
  }
}
