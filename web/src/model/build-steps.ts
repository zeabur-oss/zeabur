/**
 * A build container's log read as steps. Zeabur Builder writes one JSON event per line
 * (zeabur-oss/builder, `progress.rs`): `{vertex, name, event, time, stream, text, seconds,
 * message}`, `event` one of `started`, `log`, `done`, `cached`, `error` and `message`. A line that
 * is not such an event, from an older builder or from `buildctl` itself, is kept as it is.
 */

export type StepState = "running" | "done" | "cached" | "failed"

/** A step of the Dockerfile or the Railpack plan, or BuildKit's own work before and after them. */
export type StepKind = "own" | "prepare" | "export"

export interface LogLine {
  text: string
  stderr: boolean
  /** The name of a BuildKit step, heading its lines inside a folded group. */
  title?: boolean
}

export interface Step {
  key: string
  kind: StepKind
  name: string
  state: StepState
  /** When it started, in ms. */
  started?: number
  /** How long it took, once it ended. */
  seconds?: number
  error?: string
  lines: LogLine[]
}

export interface BuildSteps {
  /** Preparing first, then the build's own steps in the order they started, then exporting. */
  steps: Step[]
  /** Lines that belong to no step. */
  other: string[]
  /** The first step that failed. */
  failed?: { name: string; seconds?: number; message: string }
}

interface Event {
  vertex?: number
  name?: string
  event: string
  time?: string
  stream?: string | number
  text?: string
  seconds?: number
  message?: string
}

function event(line: string): Event | undefined {
  if (!line.startsWith("{")) return undefined
  try {
    const value: unknown = JSON.parse(line)
    if (typeof value !== "object" || value === null) return undefined
    const record = value as Partial<Event>
    return typeof record.event === "string" ? (record as Event) : undefined
  } catch {
    return undefined
  }
}

const EXPORT = /^(exporting|pushing|writing image|naming to|preparing build cache)/i
const PREPARE = /^(\[internal\]|load metadata|resolve image config|importing cache)/i

function kindOf(name: string): StepKind {
  if (EXPORT.test(name)) return "export"
  if (PREPARE.test(name)) return "prepare"
  return "own"
}

const handlers: Record<string, (step: Step, e: Event) => void> = {
  started: (step, e) => {
    const ms = Date.parse(e.time ?? "")
    if (Number.isFinite(ms)) step.started = ms
  },
  log: (step, e) => {
    step.lines.push({ text: e.text ?? "", stderr: e.stream === "stderr" || e.stream === 2 })
  },
  done: (step, e) => {
    step.state = "done"
    step.seconds = e.seconds
  },
  cached: (step) => {
    step.state = "cached"
  },
  error: (step, e) => {
    step.state = "failed"
    step.seconds = e.seconds
    step.error = e.message ?? ""
  },
}

function handle(vertices: Map<number, Step>, vertex: number, e: Event) {
  let step = vertices.get(vertex)
  if (!step) {
    step = { key: String(vertex), kind: "own", name: "", state: "running", lines: [] }
    vertices.set(vertex, step)
  }
  if (e.name) step.name = e.name
  handlers[e.event]?.(step, e)
}

/** The steps by vertex, in the order they first appear, and the lines of no step. */
function readEvents(lines: readonly string[]) {
  const vertices = new Map<number, Step>()
  const other: string[] = []
  for (const line of lines) {
    const e = event(line)
    if (!e) other.push(line)
    else if (e.event === "message") other.push(e.text ?? "")
    else if (typeof e.vertex === "number") handle(vertices, e.vertex, e)
  }
  return { vertices: [...vertices.values()], other }
}

/** The steps a build container's log lines tell of, in the order they were written. */
export function readBuildLog(lines: readonly string[]): BuildSteps {
  const { vertices, other } = readEvents(lines)
  const own: Step[] = []
  const prepare: Step[] = []
  const exporting: Step[] = []
  for (const step of vertices) {
    step.kind = kindOf(step.name)
    if (step.kind === "prepare") prepare.push(step)
    else if (step.kind === "export") exporting.push(step)
    else own.push(step)
  }
  const first = vertices.find((step) => step.state === "failed")
  const failed = first && { name: first.name, seconds: first.seconds, message: first.error ?? "" }
  const steps: Step[] = []
  if (prepare.length > 0) steps.push(group("prepare", prepare))
  steps.push(...own)
  if (exporting.length > 0) steps.push(group("export", exporting))
  return { steps, other, failed }
}

const rank: Record<StepState, number> = { cached: 0, done: 1, running: 2, failed: 3 }

/** BuildKit's own steps as one row: failed or running when any is, its time from the first
 * start to the last end. */
function group(kind: "prepare" | "export", members: Step[]): Step {
  const out: Step = { key: kind, kind, name: "", state: "cached", lines: [] }
  for (const step of members) {
    if (rank[step.state] > rank[out.state]) out.state = step.state
    if (step.state === "failed" && !out.error) {
      out.name = step.name
      out.error = step.error
    }
    if (step.lines.length > 0)
      out.lines.push({ text: step.name, stderr: false, title: true }, ...step.lines)
  }
  const timed = members.filter((step) => step.started !== undefined)
  if (timed.length > 0) out.started = Math.min(...timed.map((step) => step.started ?? 0))
  if (out.state === "done" || out.state === "failed") out.seconds = span(members)
  return out
}

/** Seconds from the first start to the last end, or the sum when no start is known. */
function span(members: Step[]): number {
  let first = Number.POSITIVE_INFINITY
  let last = Number.NEGATIVE_INFINITY
  let total = 0
  for (const step of members) {
    total += step.seconds ?? 0
    if (step.started === undefined) continue
    first = Math.min(first, step.started)
    last = Math.max(last, step.started + (step.seconds ?? 0) * 1000)
  }
  return Number.isFinite(first) ? (last - first) / 1000 : total
}

/** Seconds a step took, or has taken so far while it runs. */
export function stepSeconds(step: Step, now: number): number | undefined {
  if (step.state === "running")
    return step.started === undefined ? undefined : Math.max(0, (now - step.started) / 1000)
  return step.seconds
}
