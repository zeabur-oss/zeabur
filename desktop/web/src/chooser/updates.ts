import { getVersion } from "@tauri-apps/api/app"
import { invoke } from "@tauri-apps/api/core"
import { listen } from "@tauri-apps/api/event"

/** `Available` in desktop/src/update.rs. */
export interface Available {
  version: string
  current: string
  install: "inPlace" | "package" | "download"
}

/** `Failure` in desktop/src/update.rs. */
export type Failure = "notConfigured" | "failed"

export type Status =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "latest" }
  | { kind: "available"; update: Available }
  | { kind: "failed"; reason: Failure }

export interface UpdateState {
  /** This build's version; null until the app answers. */
  version: string | null
  status: Status
  /** The dialog of the available update. */
  open: boolean
  installing: boolean
  installFailed: boolean
}

/** The status a check ends in; anything but `notConfigured` is the same short failure. */
export function answer(result: { found: Available | null } | { error: unknown }): Status {
  if ("found" in result)
    return result.found ? { kind: "available", update: result.found } : { kind: "latest" }
  return { kind: "failed", reason: result.error === "notConfigured" ? "notConfigured" : "failed" }
}

const api = {
  takeCheckRequest: () => invoke<boolean>("take_check_request"),
  found: () => invoke<Available | null>("found_update"),
  check: () => invoke<Available | null>("check_update"),
  install: () => invoke<void>("install_update"),
  openReleases: () => invoke<void>("open_releases"),
}

let state: UpdateState = {
  version: null,
  status: { kind: "idle" },
  open: false,
  installing: false,
  installFailed: false,
}
const subscribers = new Set<() => void>()
let stop: (() => void) | null = null

function set(next: Partial<UpdateState>) {
  state = { ...state, ...next }
  subscribers.forEach((notify) => notify())
}

/** A check the person asked for; a newer version opens its dialog, other answers are toasts. */
export async function check() {
  if (state.status.kind === "checking") return
  set({ status: { kind: "checking" } })
  const status = await api.check().then(
    (found) => answer({ found }),
    (error: unknown) => answer({ error }),
  )
  set({ status, open: status.kind === "available", installFailed: false })
}

export function show() {
  set({ open: true, installFailed: false })
}

export function close() {
  if (!state.installing) set({ open: false })
}

export function install() {
  set({ installing: true, installFailed: false })
  api.install().catch(() => set({ installing: false, installFailed: true }))
}

export const openReleases = api.openReleases

/** Quiet on start: desktop/src/update.rs tells the chooser what it found. */
function start() {
  const found = (update: Available) => {
    if (state.status.kind === "idle") set({ status: { kind: "available", update } })
  }
  void getVersion().then((version) => set({ version }))
  void api.found().then((update) => update && found(update))
  const offFound = listen<Available>("update-available", (event) => found(event.payload))
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
