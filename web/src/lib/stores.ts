// Small external stores for shell state: the command palette, the sidebar's width (remembered in
// localStorage, which may be unavailable), the phone menu, recent palette items, and each project's
// Architecture viewport (kept for this visit, so going back to the canvas finds it as it was).
import { useSyncExternalStore } from "react"

export function store<T>(initial: T) {
  let value = initial
  const listeners = new Set<() => void>()
  const subscribe = (listener: () => void) => {
    listeners.add(listener)
    return () => {
      listeners.delete(listener)
    }
  }
  return {
    get: () => value,
    set: (next: T) => {
      value = next
      for (const listener of listeners) listener()
    },
    useValue: () => useSyncExternalStore(subscribe, () => value),
  }
}

function load(key: string): string | null {
  try {
    return window.localStorage.getItem(key)
  } catch {
    return null
  }
}

function save(key: string, value: string) {
  try {
    window.localStorage.setItem(key, value)
  } catch {
    // Storage unavailable: the choice lasts this visit.
  }
}

export const palette = store(false)
export const mobileNav = store(false)

// Two remembered widths: one outside projects, one inside a project, where the sidebar starts as
// an icon rail so the service column has room.
const sidebarKey = "engawa.sidebar"
const projectSidebarKey = "engawa.sidebar.project"
export const sidebarRail = store(load(sidebarKey) === "rail")
export const projectSidebarRail = store(load(projectSidebarKey) !== "full")

export function setSidebarRail(inProject: boolean, rail: boolean) {
  if (inProject) {
    projectSidebarRail.set(rail)
    save(projectSidebarKey, rail ? "rail" : "full")
  } else {
    sidebarRail.set(rail)
    save(sidebarKey, rail ? "rail" : "full")
  }
}

export interface Recent {
  id: string
  label: string
  hint: string
  href: string
}

const recentKey = "engawa.recent"

function loadRecent(): Recent[] {
  try {
    const parsed: unknown = JSON.parse(load(recentKey) ?? "[]")
    return Array.isArray(parsed) ? (parsed as Recent[]).slice(0, 5) : []
  } catch {
    return []
  }
}

export const recent = store<Recent[]>(loadRecent())

export function remember(item: Recent) {
  const next = [item, ...recent.get().filter((r) => r.id !== item.id)].slice(0, 5)
  recent.set(next)
  save(recentKey, JSON.stringify(next))
}

// Kinds pinned on the Objects page (`<group>/<resource>`), read at once in every project. A
// convenience: without storage the pins last this visit.
const pinnedKey = "engawa.objects.pinned"

function loadPinned(): string[] {
  try {
    const parsed: unknown = JSON.parse(load(pinnedKey) ?? "[]")
    return Array.isArray(parsed) ? parsed.filter((k): k is string => typeof k === "string") : []
  } catch {
    return []
  }
}

export const pinnedKinds = store<string[]>(loadPinned())

export function togglePinnedKind(key: string) {
  const current = pinnedKinds.get()
  const next = current.includes(key) ? current.filter((k) => k !== key) : [...current, key]
  pinnedKinds.set(next)
  save(pinnedKey, JSON.stringify(next))
}

export interface Viewport {
  x: number
  y: number
  k: number
}

const viewports = new Map<string, Viewport>()

export const canvasViewport = {
  get: (project: string) => viewports.get(project),
  set: (project: string, view: Viewport) => void viewports.set(project, view),
}
