import type { Translate } from "@/i18n"

import type { Auth, Context, ShellRead, Snapshot, SourceKind } from "./api"

export function baseName(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean)
  return parts.at(-1) ?? path
}

/** The file a context comes from, as "folder/file" so two `config` files stay apart. */
export function fileLabel(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean)
  return parts.slice(-2).join("/")
}

export const sourceKindKeys = {
  default: "desktop.sourceDefault",
  file: "desktop.sourceFile",
} as const satisfies Record<SourceKind, string>

type AuthMessages = {
  [S in Auth["state"]]: (
    auth: Extract<Auth, { state: S }>,
    shell: ShellRead,
    t: Translate,
  ) => string | null
}

const authMessages: AuthMessages = {
  ok: () => null,
  pluginMissing: (auth, shell, t) =>
    t(
      shell.state === "pending" ? "desktop.authLooking" : "desktop.authPluginMissing",
      auth.command,
    ),
  missingEntry: (auth, _, t) =>
    t(auth.what === "user" ? "desktop.authMissingUser" : "desktop.authMissingCluster", auth.name),
  fileMissing: (auth, _, t) => t("desktop.authFileMissing", fileLabel(auth.path)),
}

/** What is wrong with a context's sign-in, or null when nothing is. */
export function authText(auth: Auth, shell: ShellRead, t: Translate): string | null {
  const message = authMessages[auth.state] as (
    auth: Auth,
    shell: ShellRead,
    t: Translate,
  ) => string | null
  return message(auth, shell, t)
}

function matchesQuery(c: Context, query: string): boolean {
  return [c.name, c.server, c.user, c.file, c.cluster].some((v) => v?.toLowerCase().includes(query))
}

/** The chosen source, or null once that source is no longer in the snapshot. */
export function sourceFilter(snap: Snapshot, only: string | null): string | null {
  return snap.sources.some((s) => s.id === only) ? only : null
}

/** Contexts of the chosen source that match the search. */
export function visibleContexts(snap: Snapshot, query: string, only: string | null): Context[] {
  const q = query.trim().toLowerCase()
  return snap.contexts.filter((c) => (!only || c.sourceId === only) && matchesQuery(c, q))
}
