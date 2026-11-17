import { getVersion } from "@tauri-apps/api/app"
import { invoke } from "@tauri-apps/api/core"
import { listen } from "@tauri-apps/api/event"

/** `Available` in desktop/src/update.rs. */
export interface Available {
  version: string
  current: string
  notes: string | null
  install: "inPlace" | "package" | "download"
}

export type View =
  | { kind: "available"; update: Available; installing: boolean; error: string | null }
  | { kind: "latest"; version: string }
  | { kind: "failed"; reason: string }

export interface UpdateState {
  available: Available | null
  view: View | null
  checking: boolean
}

const api = {
  takeCheckRequest: () => invoke<boolean>("take_check_request"),
  found: () => invoke<Available | null>("found_update"),
  check: () => invoke<Available | null>("check_update"),
  install: () => invoke<void>("install_update"),
  openReleases: () => invoke<void>("open_releases"),
}

let state: UpdateState = { available: null, view: null, checking: false }
const subscribers = new Set<() => void>()
let stop: (() => void) | null = null

function set(next: Partial<UpdateState>) {
  state = { ...state, ...next }
  subscribers.forEach((notify) => notify())
}

export async function check() {
  set({ checking: true })
  try {
    const found = await api.check()
    set({
      available: found,
      view: found
        ? { kind: "available", update: found, installing: false, error: null }
        : { kind: "latest", version: await getVersion() },
    })
  } catch (error) {
    set({ view: { kind: "failed", reason: String(error) } })
  } finally {
    set({ checking: false })
  }
}

export function show(update: Available) {
  set({ view: { kind: "available", update, installing: false, error: null } })
}

export function close() {
  set({ view: null })
}

export function install(update: Available) {
  set({ view: { kind: "available", update, installing: true, error: null } })
  api
    .install()
    .catch((error: unknown) =>
      set({ view: { kind: "available", update, installing: false, error: String(error) } }),
    )
}

export const openReleases = api.openReleases

/** Quiet on start: desktop/src/update.rs tells the chooser what it found. */
function start() {
  void api.found().then((found) => found && set({ available: found }))
  const offFound = listen<Available>("update-available", (event) =>
    set({ available: event.payload }),
  )
  const checkIfAsked = async () => {
    if (await api.takeCheckRequest()) await check()
  }
  void checkIfAsked()
  const offMenu = listen("check-updates", () => void checkIfAsked())
  return () => {
    void offFound.then((off) => off())
    void offMenu.then((off) => off())
  }
}

export function subscribe(notify: () => void) {
  if (subscribers.size === 0) stop = start()
  subscribers.add(notify)
  return () => {
    subscribers.delete(notify)
    if (subscribers.size === 0) {
      stop?.()
      stop = null
    }
  }
}

export const snapshot = () => state
