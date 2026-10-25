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

// A form's objects handed to the YAML page: in sessionStorage, not the URL, as they may hold
// variable values.
const handoffKey = (project: string) => `zeabur.create.${project}`

export function handOff(project: string, yaml: string) {
  try {
    window.sessionStorage.setItem(handoffKey(project), yaml)
  } catch {
    // Storage unavailable: the YAML page opens with its template.
  }
}

export function handedOff(project: string): string | null {
  try {
    return window.sessionStorage.getItem(handoffKey(project))
  } catch {
    return null
  }
}

export function dropHandoff(project: string) {
  try {
    window.sessionStorage.removeItem(handoffKey(project))
  } catch {
    // Nothing to drop.
  }
}
