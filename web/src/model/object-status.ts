// An object's `status` as `kubectl describe` reads it: conditions first, short fields as pairs,
// the rest as YAML.
import type { Health } from "@/model/service"

export interface Condition {
  type: string
  status: string
  reason?: string
  message?: string
  lastTransitionTime?: string
}

export interface StatusParts {
  conditions: Condition[]
  /** `phase`, `replicas`, `capacity.storage`: scalars, lists of scalars and maps of scalars. */
  fields: [string, string][]
  /** What does not fit on one line, when anything. */
  nested?: Record<string, unknown>
}

type Scalar = string | number | boolean | null

const isScalar = (value: unknown): value is Scalar =>
  value === null || ["string", "number", "boolean"].includes(typeof value)

const isMap = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value)

const isEmpty = (value: unknown) =>
  value === null ||
  value === "" ||
  (Array.isArray(value) && value.length === 0) ||
  (isMap(value) && Object.keys(value).length === 0)

function asConditions(value: unknown): Condition[] | undefined {
  if (!Array.isArray(value)) return undefined
  const all = value.every(
    (c) => isMap(c) && typeof c.type === "string" && typeof c.status === "string",
  )
  return all ? (value as Condition[]) : undefined
}

export function statusParts(status: Record<string, unknown> | null | undefined): StatusParts {
  const parts: StatusParts = { conditions: [], fields: [] }
  const nested: Record<string, unknown> = {}
  const keys = Object.keys(status ?? {}).toSorted((a, b) =>
    a === "phase" ? -1 : b === "phase" ? 1 : 0,
  )
  for (const key of keys) {
    const value = status?.[key]
    if (isEmpty(value)) continue
    const conditions = key === "conditions" ? asConditions(value) : undefined
    if (conditions) parts.conditions = conditions
    else if (isScalar(value)) parts.fields.push([key, String(value)])
    else if (Array.isArray(value) && value.every(isScalar))
      parts.fields.push([key, value.map(String).join(", ")])
    else if (isMap(value) && Object.values(value).every(isScalar))
      for (const [inner, scalar] of Object.entries(value))
        parts.fields.push([`${key}.${inner}`, String(scalar)])
    else nested[key] = value
  }
  if (Object.keys(nested).length > 0) parts.nested = nested
  return parts
}

/** Conditions whose `True` is the bad news. */
const negative = /(Failure|Failed|Pressure|Unavailable|Stalled|Degraded|Error|Limited)$/

export function conditionHealth({ type, status }: Condition): Health {
  if (status !== "True" && status !== "False") return "idle"
  return (status === "True") !== negative.test(type) ? "ok" : "warn"
}

const reserved = /^(true|false|null|yes|no|on|off|y|n|~)$/i
const plain = /^[A-Za-z]([\w .,'/()-]*[\w.)])?$/

function inline(value: unknown): string {
  if (Array.isArray(value)) return "[]"
  if (isMap(value)) return "{}"
  if (typeof value !== "string") return String(value)
  return plain.test(value) && !reserved.test(value) ? value : JSON.stringify(value)
}

const isBlock = (value: unknown) => (Array.isArray(value) || isMap(value)) && !isEmpty(value)

function lines(value: unknown, depth: number): string[] {
  const pad = "  ".repeat(depth)
  if (Array.isArray(value))
    return value.flatMap((item: unknown) => {
      if (!isBlock(item)) return [`${pad}- ${inline(item)}`]
      const [first = "", ...rest] = lines(item, depth + 1)
      return [`${pad}- ${first.slice(pad.length + 2)}`, ...rest]
    })
  if (!isMap(value)) return [`${pad}${inline(value)}`]
  return Object.entries(value).flatMap(([key, item]) =>
    isBlock(item)
      ? [`${pad}${inline(key)}:`, ...lines(item, Array.isArray(item) ? depth : depth + 1)]
      : [`${pad}${inline(key)}: ${inline(item)}`],
  )
}

/** Read-only YAML; strings that a YAML reader would take for something else are quoted. */
export function toYaml(value: unknown): string {
  return `${lines(value, 0).join("\n")}\n`
}
