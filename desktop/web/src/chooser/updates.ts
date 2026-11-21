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

/** The answer of the last check the person asked for. */
export type Check =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "latest" }
  | { kind: "found" }
  | { kind: "failed"; reason: Failure }

export interface UpdateState {
  /** This build's version; null until the app answers. */
  version: string | null
  /** The newer version a check found; a later failed check keeps it. */
  update: Available | null
  check: Check
  /** The dialog of the update. */
  open: boolean
  installing: boolean
  installFailed: boolean
}

type Settled = Pick<UpdateState, "check" | "update" | "open">

/** The state a check ends in; anything but `notConfigured` is the same short failure. */
export function settle(
  update: Available | null,
  result: { found: Available | null } | { error: unknown },
): Settled {
  if (!("found" in result)) {
    const reason = result.error === "notConfigured" ? "notConfigured" : "failed"
    return { check: { kind: "failed", reason }, update, open: false }
  }
  if (!result.found) return { check: { kind: "latest" }, update: null, open: false }
  return { check: { kind: "found" }, update: result.found, open: true }
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
  update: null,
  check: { kind: "idle" },
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
  if (state.check.kind === "checking") return
  set({ check: { kind: "checking" } })
  const settled = await api.check().then(
    (found) => settle(state.update, { found }),
    (error: unknown) => settle(state.update, { error }),
  )
  set({ ...settled, installFailed: false })
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
    if (state.check.kind === "idle") set({ update })
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
