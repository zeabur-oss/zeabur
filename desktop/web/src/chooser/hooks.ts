import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { listen } from "@tauri-apps/api/event"
import { useEffect } from "react"

import { api } from "./api"

const snapshotKey = ["snapshot"]

/** The kubeconfig snapshot. */
export function useSnapshot() {
  const queryClient = useQueryClient()
  const { data: snap } = useQuery({ queryKey: snapshotKey, queryFn: api.snapshot })

  useEffect(() => {
    // The login shell's environment arrives after the first snapshot and can find more plugins.
    const unlisten = listen("shell-env", () =>
      queryClient.invalidateQueries({ queryKey: snapshotKey }),
    )
    return () => {
      void unlisten.then((stop) => stop())
    }
  }, [queryClient])

  return snap
}

/** The changes to the sources and the open action; each one refreshes the snapshot when it is done. */
export function useChanges() {
  const queryClient = useQueryClient()
  const onSuccess = () => queryClient.invalidateQueries({ queryKey: snapshotKey })
  return {
    addPaths: useMutation({ mutationFn: api.addPaths, onSuccess }),
    removeSource: useMutation({ mutationFn: api.removeSource, onSuccess }),
    open: useMutation({
      mutationFn: (p: { key: string; name: string; lang: string }) =>
        api.open(p.key, p.name, p.lang),
      onSuccess,
    }),
  }
}
