// A screenshot manifest (docs/shots.yaml, docs/internal/screenshots.md), as JSON from
// `yq -o=json`. Run by scripts/docs-shots.sh: `bun e2e/shots/manifest.ts <shots.json> [name]`
// checks it and prints its settings and each distinct demo server (its app and environment) as
// one line of JSON.
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
  until?: string
  viewport: Viewport
  env: Record<string, string>
  locales: Locale[]
  themes: Theme[]
  fresh: boolean
}

export const locales = { en: "en-US", "zh-TW": "zh-TW", "zh-CN": "zh-CN" }
export const themes = ["light", "dark"] as const
export type Locale = keyof typeof locales
export type Theme = (typeof themes)[number]

// `cover` is the plate of a README cover (design-system covers/): the window keeps its size.
export const viewports = {
  desktop: { width: 1440, height: 900 },
  phone: { width: 390, height: 844 },
  cover: { width: 544, height: 544 },
}
export type Viewport = keyof typeof viewports

// A manifest that names `out` holds images kept in git: the run writes only the images, to
// <out>/<file>. Without `out`, the run also writes manifest.json and the release's tar. `themes` is
// the themes of a shot that lists none.
export type Settings = {
  out?: string
  file: string
  scale: number
  budget: number
  clean: boolean
  themes: Theme[]
}

export type Manifest = Settings & { shots: Shot[] }

const settingKeys = ["out", "file", "scale", "budget", "clean", "themes", "shots"]

const shotKeys = [
  "id",
  "app",
  "path",
  "wait",
  "steps",
  "clip",
  "until",
  "viewport",
  "env",
  "locales",
  "themes",
  "fresh",
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

function shot(value: unknown, index: number, defaultThemes: Theme[]): Shot {
  if (!isRecord(value)) throw new Error(`shot ${index}: expected a mapping`)
  const at = typeof value.id === "string" ? value.id : `shot ${index}`
  for (const key of Object.keys(value)) check(shotKeys.includes(key), `${at}: unknown key "${key}"`)
  const { id, app = "zeabur", path, wait, clip, until, viewport = "desktop", steps = [] } = value
  const { locales: shotLocales = Object.keys(locales), themes: shotThemes = defaultThemes } = value
  check(
    typeof id === "string" && /^[a-z0-9]+(-[a-z0-9]+)*$/.test(id),
    `${at}: id must be kebab-case`,
  )
  check(apps.includes(app as App), `${at}: app must be ${apps.join(" or ")}`)
  check(typeof path === "string" && path.startsWith("/"), `${at}: path must start with /`)
  check(isSelector(wait), `${at}: wait must be a CSS selector`)
  check(isSelector(clip), `${at}: clip must be a CSS selector`)
  check(isSelector(until), `${at}: until must be a CSS selector`)
  check(until === undefined || clip !== undefined, `${at}: until needs clip`)
  check(
    Object.keys(viewports).includes(viewport as string),
    `${at}: viewport must be ${Object.keys(viewports).join(", ")}`,
  )
  check(
    value.fresh === undefined || typeof value.fresh === "boolean",
    `${at}: fresh must be true or false`,
  )
  if (!Array.isArray(steps)) throw new Error(`${at}: steps must be a list`)
  return {
    id: id as string,
    app: app as App,
    path: path as string,
    wait: wait as string | undefined,
    steps: steps.map((s, i) => step(s, `${at}: step ${i + 1}`)),
    clip: clip as string | undefined,
    until: until as string | undefined,
    viewport: viewport as Shot["viewport"],
    env: environment(value.env, at),
    locales: subset(shotLocales, Object.keys(locales) as Locale[], `${at}: locales`),
    themes: subset(shotThemes, themes, `${at}: themes`),
    fresh: value.fresh === true,
  }
}

/** The demo server a shot needs as one canonical line, for grouping shots by server. */
export function serverKey({ app, env }: Pick<Shot, "app" | "env">): string {
  return JSON.stringify({ app, env: Object.fromEntries(Object.entries(env).sort()) })
}

/** The servers a shot is taken on: the one it shares with the shots that need the same, or with
 * `fresh`, a new one for each locale and theme, since its steps change what the demo holds. */
export function serversOf(shot: Shot): string[] {
  if (!shot.fresh) return [serverKey(shot)]
  const env = Object.fromEntries(Object.entries(shot.env).sort())
  return shot.locales.flatMap((locale) =>
    shot.themes.map((theme) =>
      JSON.stringify({ app: shot.app, env, shot: shot.id, project: `${locale}/${theme}` }),
    ),
  )
}

function fileName(file: unknown, shots: Shot[]): string {
  const name = typeof file === "string" ? file : ""
  check(
    name.endsWith(".avif") &&
      name.includes("{id}") &&
      !/[{}]/.test(name.replace(/\{(id|locale|theme)\}/g, "")),
    "file must end in .avif and hold {id}; it may hold {locale} and {theme}",
  )
  for (const s of shots) {
    check(name.includes("{locale}") || s.locales.length === 1, `${s.id}: file needs {locale}`)
    check(name.includes("{theme}") || s.themes.length === 1, `${s.id}: file needs {theme}`)
  }
  return name
}

function settings(value: Record<string, unknown>, shots: Shot[], themes: Theme[]): Settings {
  for (const key of Object.keys(value)) check(settingKeys.includes(key), `unknown key "${key}"`)
  const { out, file = "{locale}/{theme}/{id}.avif", scale = 2, budget = 150, clean = false } = value
  check(
    out === undefined || (typeof out === "string" && out !== "" && !out.startsWith("/")),
    "out must be a path relative to the repository",
  )
  check(scale === 1 || scale === 2, "scale must be 1 or 2")
  check(Number.isInteger(budget) && (budget as number) > 0, "budget must be a number of KB")
  check(typeof clean === "boolean", "clean must be true or false")
  check(!clean || out !== undefined, "clean needs out")
  return {
    out: out as string | undefined,
    file: fileName(file, shots),
    scale: scale as number,
    budget: budget as number,
    clean: clean as boolean,
    themes,
  }
}

/** A manifest is a list of shots, or a mapping of settings with the shots in `shots`. */
export function readManifest(file: string): Manifest {
  const data: unknown = JSON.parse(readFileSync(file, "utf8"))
  const top = Array.isArray(data) ? { shots: data } : data
  if (!isRecord(top) || !Array.isArray(top.shots)) throw new Error("expected a list of shots")
  const defaultThemes = subset(top.themes ?? ["light"], themes, "themes")
  const shots = top.shots.map((s, i) => shot(s, i, defaultThemes))
  const seen = new Set<string>()
  for (const { id } of shots) {
    if (seen.has(id)) throw new Error(`${id}: duplicate id`)
    seen.add(id)
  }
  return { ...settings(top, shots, defaultThemes), shots }
}

if (import.meta.main) {
  try {
    const { shots, ...rest } = readManifest(process.argv[2])
    console.log(JSON.stringify({ ...rest, servers: [...new Set(shots.flatMap(serversOf))] }))
  } catch (error) {
    console.error(`${process.argv[3] ?? process.argv[2]}: ${(error as Error).message}`)
    process.exit(1)
  }
}
