import type { Translate } from "@/i18n"

import type { Auth, Context, Reach, ShellRead, Snapshot, SourceKind } from "./api"

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
  folder: "desktop.sourceFolder",
  pasted: "desktop.sourcePasted",
  file: "desktop.sourceFile",
} as const satisfies Record<SourceKind, string>

const reachStates = {
  reachable: { variant: "ok", key: "desktop.reachOk" },
  unresolved: { variant: "fail", key: "desktop.reachUnresolved" },
  refused: { variant: "fail", key: "desktop.reachRefused" },
  timedOut: { variant: "warn", key: "desktop.reachTimeout" },
  noAddress: { variant: "fail", key: "desktop.reachNone" },
} as const satisfies Record<Reach["state"], { variant: string; key: string }>

export interface ReachView {
  variant: "idle" | "ok" | "fail" | "warn"
  text: string
}

export function reachView(reach: Reach | undefined, t: Translate): ReachView {
  if (!reach) return { variant: "idle", text: t("desktop.reachChecking") }
  const { variant, key } = reachStates[reach.state]
  const millis = reach.state === "reachable" ? ` ${reach.millis} ms` : ""
  return { variant, text: t(key) + millis }
}

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

/** A screenshot scenario such as `lang=en;q=prod`. */
export function parseScenario(text: string | null): Record<string, string> {
  const pairs = (text ?? "").split(";").map((part) => part.split("="))
  return Object.fromEntries(pairs.filter(([key]) => key).map(([key, value]) => [key, value ?? ""]))
}

function matchesQuery(c: Context, query: string): boolean {
  return [c.name, c.server, c.user, c.file, c.cluster].some((v) => v?.toLowerCase().includes(query))
}

/** Contexts of the chosen source that match the search, favourites first. */
export function visibleContexts(snap: Snapshot, query: string, only: string | null): Context[] {
  const q = query.trim().toLowerCase()
  const favourites = new Set(snap.favourites)
  const inSource = (c: Context) => !only || c.sourceId === only
  const list = snap.contexts.filter((c) => inSource(c) && matchesQuery(c, q))
  return list.toSorted((a, b) => Number(favourites.has(b.key)) - Number(favourites.has(a.key)))
}

/** The recently opened contexts, shown only while no search or source filter is active. */
export function recentContexts(snap: Snapshot, query: string, only: string | null): Context[] {
  if (query || only) return []
  const byKey = new Map(snap.contexts.map((c) => [c.key, c]))
  return snap.recents.flatMap((key) => byKey.get(key) ?? [])
}
