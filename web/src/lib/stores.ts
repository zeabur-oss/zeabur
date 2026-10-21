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

export function load(key: string): string | null {
  try {
    return window.localStorage.getItem(key)
  } catch {
    return null
  }
}

export function save(key: string, value: string) {
  try {
    window.localStorage.setItem(key, value)
  } catch {
    // Storage unavailable: the choice lasts this visit.
  }
}

export const palette = store(false)
export const mobileNav = store(false)

/** `initial` until the person opens or closes it. */
export function remembered(key: string, initial: boolean) {
  const value = store(load(key) === null ? initial : load(key) === "open")
  return {
    ...value,
    set: (open: boolean) => {
      value.set(open)
      save(key, open ? "open" : "closed")
    },
  }
}

export const projectsAsList = remembered("zeabur.projects.list", true)

const viewKey = "zeabur.project.view"

/** The project view last chosen, map or list; absent until the person picks one. */
export const projectView = (() => {
  const saved = load(viewKey)
  const value = store<"map" | "list" | undefined>(
    saved === "map" || saved === "list" ? saved : undefined,
  )
  return {
    ...value,
    set: (view: "map" | "list") => {
      value.set(view)
      save(viewKey, view)
    },
  }
})()

export interface Recent {
  id: string
  label: string
  hint: string
  href: string
}

const recentKey = "zeabur.recent"

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

// Pinned kinds, as `<group>/<resource>`.
const pinnedKey = "zeabur.objects.pinned"

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
