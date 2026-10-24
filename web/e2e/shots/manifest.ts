// The screenshot manifest (docs/shots.yaml), as JSON from `yq -o=json`. Run by
// scripts/docs-shots.sh: `bun e2e/shots/manifest.ts <shots.json> [name]` checks it and prints each
// distinct demo environment as one line of JSON.
import { readFileSync } from "node:fs"

export type Step =
  | { click: string }
  | { fill: string; value: string }
  | { hover: string }
  | { press: string }

export type Shot = {
  id: string
  path: string
  wait?: string
  steps: Step[]
  clip?: string
  viewport: "desktop" | "phone"
  env: Record<string, string>
}

export const viewports = {
  desktop: { width: 1440, height: 900 },
  phone: { width: 390, height: 844 },
}

const shotKeys = ["id", "path", "wait", "steps", "clip", "viewport", "env"]
const stepKeys: Record<string, string[]> = {
  click: ["click"],
  fill: ["fill", "value"],
  hover: ["hover"],
  press: ["press"],
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

function shot(value: unknown, index: number): Shot {
  if (!isRecord(value)) throw new Error(`shot ${index}: expected a mapping`)
  const at = typeof value.id === "string" ? value.id : `shot ${index}`
  for (const key of Object.keys(value)) check(shotKeys.includes(key), `${at}: unknown key "${key}"`)
  const { id, path, wait, clip, viewport = "desktop", steps = [] } = value
  check(
    typeof id === "string" && /^[a-z0-9]+(-[a-z0-9]+)*$/.test(id),
    `${at}: id must be kebab-case`,
  )
  check(typeof path === "string" && path.startsWith("/"), `${at}: path must start with /`)
  check(isSelector(wait), `${at}: wait must be a CSS selector`)
  check(isSelector(clip), `${at}: clip must be a CSS selector`)
  check(viewport === "desktop" || viewport === "phone", `${at}: viewport must be desktop or phone`)
  if (!Array.isArray(steps)) throw new Error(`${at}: steps must be a list`)
  return {
    id: id as string,
    path: path as string,
    wait: wait as string | undefined,
    steps: steps.map((s, i) => step(s, `${at}: step ${i + 1}`)),
    clip: clip as string | undefined,
    viewport: viewport as Shot["viewport"],
    env: environment(value.env, at),
  }
}

/** The environment as one canonical line, for grouping shots by demo server. */
export function envKey(env: Record<string, string>): string {
  return JSON.stringify(Object.fromEntries(Object.entries(env).sort()))
}

export function readManifest(file: string): Shot[] {
  const data: unknown = JSON.parse(readFileSync(file, "utf8"))
  if (!Array.isArray(data) || data.length === 0) throw new Error("expected a list of shots")
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
    for (const key of new Set(shots.map((s) => envKey(s.env)))) console.log(key)
  } catch (error) {
    console.error(`${process.argv[3] ?? process.argv[2]}: ${(error as Error).message}`)
    process.exit(1)
  }
}
