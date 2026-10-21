// What this browser last read from the cluster, so a page can still name the cluster and its
// projects while the cluster cannot be reached. Kept per viewer; never a source of truth.
import type { Schemas } from "@/api/client"
import { load, save } from "@/lib/stores"

const key = "zeabur.lastKnown"

export interface LastKnown {
  /** When the cluster last answered (ms since the epoch). */
  at?: number
  cluster?: string
  projects?: Schemas["ProjectSummary"][]
}

export function lastKnown(): LastKnown {
  try {
    const value: unknown = JSON.parse(load(key) ?? "{}")
    return typeof value === "object" && value !== null ? (value as LastKnown) : {}
  } catch {
    return {}
  }
}

export function rememberKnown(patch: LastKnown) {
  const next = { ...lastKnown(), ...patch }
  if (JSON.stringify(next) !== load(key)) save(key, JSON.stringify(next))
}
