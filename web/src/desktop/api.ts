import { invoke } from "@tauri-apps/api/core"

export type SourceKind = "default" | "file" | "folder" | "pasted"

export interface Source {
  id: string
  kind: SourceKind
  path: string
  files: number
  contexts: number
  errors: { file: string; reason: string }[]
  missing: boolean
}

export type Auth =
  | { state: "ok"; method: string }
  | { state: "shellPathOnly"; command: string }
  | { state: "pluginMissing"; command: string; hint: string | null }
  | { state: "missingEntry"; what: string; name: string }
  | { state: "fileMissing"; path: string }

export interface Context {
  key: string
  sourceId: string
  file: string
  name: string
  cluster: string
  server: string | null
  user: string | null
  namespace: string | null
  current: boolean
  sameNameElsewhere: number
  auth: Auth
}

export type Reach =
  | { state: "reachable"; millis: number }
  | { state: "noAddress" }
  | { state: "unresolved"; host: string }
  | { state: "refused" }
  | { state: "timedOut" }

export interface Snapshot {
  sources: Source[]
  contexts: Context[]
  favourites: string[]
  recents: string[]
  shellPathUsed: boolean
}

export const api = {
  snapshot: () => invoke<Snapshot>("snapshot"),
  addPaths: (paths: string[]) => invoke<void>("add_paths", { paths }),
  addPasted: (name: string, yaml: string) => invoke<void>("add_pasted", { name, yaml }),
  removeSource: (id: string) => invoke<void>("remove_source", { id }),
  toggleFavourite: (key: string) => invoke<void>("toggle_favourite", { key }),
  probe: (targets: [string, string | null][]) =>
    invoke<{ key: string; reach: Reach }[]>("probe", { targets }),
  open: (key: string, name: string, lang: string) =>
    invoke<void>("open_cluster", { key, name, lang }),
  scenario: () => invoke<string | null>("scenario"),
}

/** Broken entries are shown but cannot be opened. */
export function blocked(auth: Auth): boolean {
  return (
    auth.state === "pluginMissing" || auth.state === "missingEntry" || auth.state === "fileMissing"
  )
}
