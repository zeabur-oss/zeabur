import { listen } from "@tauri-apps/api/event"
import { getCurrentWebview } from "@tauri-apps/api/webview"
import { useEffect, useState } from "react"

import type { Locale } from "@/i18n"

import { api, type Reach, type Snapshot } from "./api"
import { parseScenario } from "./model"

/** The kubeconfig snapshot, and the reachability of each server as the probes answer. */
export function useSnapshot() {
  const [snap, setSnap] = useState<Snapshot | null>(null)
  const [reach, setReach] = useState<Record<string, Reach>>({})

  async function reload() {
    const next = await api.snapshot()
    setSnap(next)
    setReach({})
    for (const c of next.contexts) {
      void api.probe([[c.key, c.server]]).then((results) => {
        for (const r of results) setReach((prev) => ({ ...prev, [r.key]: r.reach }))
      })
    }
    return next
  }

  useEffect(() => {
    // The login shell's environment arrives after the first snapshot and can find more plugins.
    const unlisten = listen("shell-env", () => void reload())
    return () => {
      void unlisten.then((stop) => stop())
    }
    // Runs once: the listener belongs to the window.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  return { snap, reach, reload }
}

/** Files dropped on the window are added as sources; `dropping` is true while they hover. */
export function useDropTarget(reload: () => Promise<unknown>) {
  const [dropping, setDropping] = useState(false)

  useEffect(() => {
    const unlisten = getCurrentWebview().onDragDropEvent((event) => {
      const payload = event.payload
      setDropping(payload.type === "enter" || payload.type === "over")
      if (payload.type === "drop") void api.addPaths(payload.paths).then(reload)
    })
    return () => {
      void unlisten.then((stop) => stop())
    }
    // Runs once: the listener belongs to the window.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  return { dropping, setDropping }
}

export interface ScenarioActions {
  locale: Locale
  setLocale: (locale: Locale) => void
  setQuery: (query: string) => void
  setOnly: (id: string | null) => void
  showPaste: () => void
  showDrop: () => void
}

type Step = (value: string, loaded: Snapshot, actions: ScenarioActions) => void

const steps: Record<string, Step> = {
  lang: (value, _, a) => a.setLocale(value as Locale),
  q: (value, _, a) => a.setQuery(value),
  paste: (_, __, a) => a.showPaste(),
  drop: (_, __, a) => a.showDrop(),
  only: (value, loaded, a) => a.setOnly(loaded.sources[Number(value)]?.id ?? null),
}

function applyScenario(scenario: Record<string, string>, loaded: Snapshot, a: ScenarioActions) {
  for (const [key, value] of Object.entries(scenario)) {
    if (value) steps[key]?.(value, loaded, a)
  }
}

function openScenarioCluster(scenario: Record<string, string>, loaded: Snapshot, locale: Locale) {
  const picked = loaded.contexts.find((c) => c.name === scenario.open)
  if (picked) void api.open(picked.key, picked.name, scenario.lang ?? locale)
}

/** Screenshots set the window up through a scenario string; a normal run has none. */
async function runScenario(loaded: Snapshot, actions: ScenarioActions) {
  const scenario = parseScenario(await api.scenario())
  applyScenario(scenario, loaded, actions)
  openScenarioCluster(scenario, loaded, actions.locale)
}

export function useScenario(reload: () => Promise<Snapshot>, actions: ScenarioActions) {
  useEffect(() => {
    void reload().then((loaded) => runScenario(loaded, actions))
    // Runs once: the scenario belongs to the window.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])
}
