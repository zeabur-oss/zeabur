import { invoke } from "@tauri-apps/api/core"

export type SourceKind = "default" | "file"

export interface Source {
  id: string
  kind: SourceKind
  path: string
  contexts: number
  error: string | null
  missing: boolean
}

export type Auth =
  | { state: "ok"; method: string }
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
  auth: Auth
}

export type ShellRead =
  | { state: "pending" }
  | { state: "done" }
  | { state: "skipped" }
  | { state: "failed"; shell: string; timedOut: boolean; detail: string }

export interface Snapshot {
  sources: Source[]
  contexts: Context[]
  shell: ShellRead
}

export const api = {
  snapshot: () => invoke<Snapshot>("snapshot"),
  addPaths: (paths: string[]) => invoke<void>("add_paths", { paths }),
  removeSource: (id: string) => invoke<void>("remove_source", { id }),
  open: (key: string, name: string, lang: string) =>
    invoke<void>("open_cluster", { key, name, lang }),
}

/** Broken entries are shown but cannot be opened. */
export function blocked(auth: Auth): boolean {
  return (
    auth.state === "pluginMissing" || auth.state === "missingEntry" || auth.state === "fileMissing"
  )
}
