// What this browser last read from the cluster, so a page can still name the cluster and its
// projects while the cluster cannot be reached. Kept per viewer; never a source of truth.
import * as z from "zod/mini"

import type { Schemas } from "@/api/client"
import { load, save } from "@/lib/stores"

const key = "zeabur.lastKnown"

const stored = z.object({
  /** When the cluster last answered (ms since the epoch). */
  at: z.optional(z.number()),
  cluster: z.optional(z.string()),
  projects: z.optional(z.array(z.custom<Schemas.ProjectSummary>())),
})

export type LastKnown = z.output<typeof stored>

export function lastKnown(): LastKnown {
  try {
    return stored.safeParse(JSON.parse(load(key) ?? "{}")).data ?? {}
  } catch {
    return {}
  }
}

export function rememberKnown(patch: LastKnown) {
  const next = { ...lastKnown(), ...patch }
  if (JSON.stringify(next) !== load(key)) save(key, JSON.stringify(next))
}
