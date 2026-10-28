import type { Schemas } from "@/api/client"
import { store } from "@/lib/stores"

export type AddWhat =
  | "image"
  | "git"
  | "upload"
  | "compose"
  | "template"
  | "objectStorage"
  | Schemas["DatabaseEngine"]

/** The "New" form open over any page, so the menu, the palette and the empty project share it. */
export const adding = store<{ project: string; what: AddWhat } | null>(null)

export const openAdd = (project: string, what: AddWhat) => adding.set({ project, what })

// A form's objects handed to the YAML page: kept in memory, not the URL or storage, as they may
// hold variable values.
let handoff: { project: string; yaml: string } | null = null

export function handOff(project: string, yaml: string) {
  handoff = { project, yaml }
}

export function handedOff(project: string): string | null {
  return handoff?.project === project ? handoff.yaml : null
}

export function dropHandoff(project: string) {
  if (handoff?.project === project) handoff = null
}
