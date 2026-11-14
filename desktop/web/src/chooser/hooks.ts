import { useMutation, useQueries, useQuery, useQueryClient } from "@tanstack/react-query"
import { listen } from "@tauri-apps/api/event"
import { getCurrentWebview } from "@tauri-apps/api/webview"
import { useEffect, useState } from "react"

import { api, type Reach } from "./api"

const snapshotKey = ["snapshot"]

/** The kubeconfig snapshot, and the reachability of each server as the probes answer. */
export function useSnapshot() {
  const queryClient = useQueryClient()
  const { data: snap } = useQuery({ queryKey: snapshotKey, queryFn: api.snapshot })
  const probes = useQueries({
    queries: (snap?.contexts ?? []).map((c) => ({
      queryKey: ["probe", c.key, c.server],
      queryFn: async () => (await api.probe([[c.key, c.server]]))[0]?.reach ?? null,
      staleTime: Infinity,
    })),
  })
  const reach: Record<string, Reach> = {}
  snap?.contexts.forEach((c, i) => {
    const found = probes[i]?.data
    if (found) reach[c.key] = found
  })

  useEffect(() => {
    // The login shell's environment arrives after the first snapshot and can find more plugins.
    const unlisten = listen("shell-env", () =>
      queryClient.invalidateQueries({ queryKey: snapshotKey }),
    )
    return () => {
      void unlisten.then((stop) => stop())
    }
  }, [queryClient])

  return { snap, reach }
}

/** The changes to the sources and the contexts; each one refreshes the snapshot when it is done. */
export function useChanges() {
  const queryClient = useQueryClient()
  const onSuccess = () => queryClient.invalidateQueries({ queryKey: snapshotKey })
  return {
    addPaths: useMutation({ mutationFn: api.addPaths, onSuccess }),
    addPasted: useMutation({
      mutationFn: (p: { name: string; yaml: string }) => api.addPasted(p.name, p.yaml),
      onSuccess,
    }),
    removeSource: useMutation({ mutationFn: api.removeSource, onSuccess }),
    toggleFavourite: useMutation({ mutationFn: api.toggleFavourite, onSuccess }),
    open: useMutation({
      mutationFn: (p: { key: string; name: string; lang: string }) =>
        api.open(p.key, p.name, p.lang),
      onSuccess,
    }),
  }
}

/** Files dropped on the window are added as sources; `dropping` is true while they hover. */
export function useDropTarget() {
  const [dropping, setDropping] = useState(false)
  const { mutate: addPaths } = useChanges().addPaths

  useEffect(() => {
    const unlisten = getCurrentWebview().onDragDropEvent((event) => {
      const payload = event.payload
      setDropping(payload.type === "enter" || payload.type === "over")
      if (payload.type === "drop") addPaths(payload.paths)
    })
    return () => {
      void unlisten.then((stop) => stop())
    }
  }, [addPaths])

  return { dropping }
}
