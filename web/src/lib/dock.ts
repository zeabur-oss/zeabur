// The terminals docked under every page: which are open and which is shown. Kept here, outside the
// pages, so a terminal lives on while the person moves between pages and services.
import { load, save, store } from "@/lib/stores"

export interface ServiceRef {
  kind: string
  name: string
}

export interface DatabaseRef {
  engine: string
  name: string
}

export type DockTab =
  | {
      id: string
      kind: "kubectl"
      project: string
      /** Bumped by each explicit ask, which attaches or plans a tools pod without another click. */
      asked: number
      /** The tools pod the last ask named. */
      pod?: string
      /** Typed into the shell once it opens, for the person to read and run with Enter. */
      command?: string
    }
  | {
      id: string
      kind: "shell"
      project: string
      service: ServiceRef
      pod?: string
      /** Named in the tab when the replica has more than one. */
      container?: string
      /** A database's members instead of a service's replicas: `service` is its object. */
      database?: DatabaseRef
    }

export interface DockState {
  open: boolean
  tabs: DockTab[]
  active?: string
  /** Bumped when the shown terminal should take the keyboard. */
  focus: number
}

export const dock = store<DockState>({ open: false, tabs: [], focus: 0 })

const heightKey = "zeabur.dock.height"
export const dockMinHeight = 160
const dockDefaultHeight = 320

export const dockHeight = (() => {
  const saved = Number(load(heightKey))
  const value = store(Number.isFinite(saved) && saved >= dockMinHeight ? saved : dockDefaultHeight)
  return {
    ...value,
    set: (height: number) => {
      value.set(height)
      save(heightKey, String(height))
    },
  }
})()

/** When each tab's session opened, the expiry of the credential it was opened with. */
export const dockExpiries = store<Record<string, string>>({})

export function setExpiry(id: string, expiresAt: string | null | undefined) {
  const { [id]: _, ...rest } = dockExpiries.get()
  dockExpiries.set(expiresAt ? { ...rest, [id]: expiresAt } : rest)
}

const update = (patch: Partial<DockState>) => dock.set({ ...dock.get(), ...patch })

export function showDock(open: boolean) {
  const state = dock.get()
  update({ open, focus: open ? state.focus + 1 : state.focus })
}

export const toggleDock = () => showDock(!dock.get().open)

export function selectTab(id: string) {
  update({ active: id, open: true, focus: dock.get().focus + 1 })
}

const shellId = (project: string, service: ServiceRef) =>
  `shell:${project}/${service.kind}/${service.name}`

/** Opens a terminal in the service, or shows the one already open. */
export function openShell(
  project: string,
  service: ServiceRef,
  pod?: string,
  database?: DatabaseRef,
) {
  const state = dock.get()
  const id = shellId(project, service)
  const tabs = state.tabs.some((tab) => tab.id === id)
    ? state.tabs.map((tab) =>
        tab.id === id && tab.kind === "shell" && pod ? { ...tab, pod } : tab,
      )
    : [...state.tabs, { id, kind: "shell" as const, project, service, pod, database }]
  dock.set({ tabs, active: id, open: true, focus: state.focus + 1 })
}

/** Opens a terminal in the database's server, or shows the one already open. */
export const openDatabaseShell = (
  project: string,
  database: DatabaseRef & { kind: string },
  pod?: string,
) =>
  openShell(project, { kind: database.kind, name: database.name }, pod, {
    engine: database.engine,
    name: database.name,
  })

/** Opens the project's kubectl terminal, or shows it. An ask attaches to the tools pod named, or a
 * running one, or plans one; without, the tab lists the person's tools pods. A `command` is typed
 * in once the shell opens, and not run. */
export function openKubectl(
  project: string,
  { ask = true, pod, command }: { ask?: boolean; pod?: string; command?: string } = {},
) {
  const state = dock.get()
  const id = `kubectl:${project}`
  const found = state.tabs.find((tab) => tab.id === id)
  const tabs = found
    ? state.tabs.map((tab) =>
        tab.id === id && tab.kind === "kubectl" && ask
          ? { ...tab, asked: tab.asked + 1, pod, command: command ?? tab.command }
          : tab,
      )
    : [...state.tabs, { id, kind: "kubectl" as const, project, asked: ask ? 1 : 0, pod, command }]
  dock.set({ tabs, active: id, open: true, focus: state.focus + 1 })
}

/** Its command is in the shell now. */
export function commandTyped(id: string) {
  const state = dock.get()
  update({
    tabs: state.tabs.map((tab) =>
      tab.id === id && tab.kind === "kubectl" ? { ...tab, command: undefined } : tab,
    ),
  })
}

export function updateShell(id: string, patch: { pod?: string; container?: string }) {
  const state = dock.get()
  update({
    tabs: state.tabs.map((tab) =>
      tab.id === id && tab.kind === "shell" ? { ...tab, ...patch } : tab,
    ),
  })
}

/** The tab beside it is shown next; the dock stays open, empty, after the last. */
export function closeTab(id: string) {
  const state = dock.get()
  const index = state.tabs.findIndex((tab) => tab.id === id)
  if (index < 0) return
  const tabs = state.tabs.filter((tab) => tab.id !== id)
  const active =
    state.active === id ? (tabs[Math.min(index, tabs.length - 1)]?.id ?? undefined) : state.active
  setExpiry(id, undefined)
  dock.set({ ...state, tabs, active, focus: state.focus + 1 })
}

const mac = typeof navigator !== "undefined" && /Mac|iPhone|iPad|iPod/.test(navigator.platform)

/** As the keys show it. */
export const dockShortcut = mac ? "⌃`" : "Ctrl `"

/** Ctrl+` (VS Code's): on the key left of 1 whatever the layout prints there, so a plain backtick
 * still types. */
export function isDockShortcut(
  event: Pick<KeyboardEvent, "ctrlKey" | "metaKey" | "altKey" | "shiftKey" | "code" | "key">,
): boolean {
  return (
    event.ctrlKey &&
    !event.metaKey &&
    !event.altKey &&
    !event.shiftKey &&
    (event.code === "Backquote" || event.key === "`")
  )
}
