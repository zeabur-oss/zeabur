import { listen } from "@tauri-apps/api/event"
import { getCurrentWebview } from "@tauri-apps/api/webview"
import { useEffect, useEffectEvent, useState } from "react"

import { api, type Reach, type Snapshot } from "./api"

/** The kubeconfig snapshot, and the reachability of each server as the probes answer. */
export function useSnapshot() {
  const [snap, setSnap] = useState<Snapshot | null>(null)
  const [reach, setReach] = useState<Record<string, Reach>>({})

  const reload = useEffectEvent(async () => {
    const next = await api.snapshot()
    setSnap(next)
    setReach({})
    for (const c of next.contexts) {
      void api.probe([[c.key, c.server]]).then((results) => {
        for (const r of results) setReach((prev) => ({ ...prev, [r.key]: r.reach }))
      })
    }
    return next
  })

  useEffect(() => {
    void reload()
    // The login shell's environment arrives after the first snapshot and can find more plugins.
    const unlisten = listen("shell-env", () => void reload())
    return () => {
      void unlisten.then((stop) => stop())
    }
  }, [])

  return { snap, reach, reload }
}

/** Files dropped on the window are added as sources; `dropping` is true while they hover. */
export function useDropTarget(reload: () => Promise<unknown>) {
  const [dropping, setDropping] = useState(false)
  const onDrop = useEffectEvent((paths: string[]) => void api.addPaths(paths).then(reload))

  useEffect(() => {
    const unlisten = getCurrentWebview().onDragDropEvent((event) => {
      const payload = event.payload
      setDropping(payload.type === "enter" || payload.type === "over")
      if (payload.type === "drop") onDrop(payload.paths)
    })
    return () => {
      void unlisten.then((stop) => stop())
    }
  }, [])

  return { dropping }
}
