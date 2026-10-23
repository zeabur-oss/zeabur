// The kubectl terminal for the whole cluster (`/api/v1/cluster/tools`), for cluster administrators.
import { queryOptions } from "@tanstack/react-query"

import { api, isStatus, unwrap, type Schemas } from "@/api/client"
import { queryClient } from "@/api/queries"

export type ClusterToolsPlan = Schemas["ClusterToolsPlan"]

export const clusterToolsQuery = queryOptions({
  queryKey: ["cluster", "tools"],
  queryFn: async ({ signal }) => unwrap(await api.GET("/api/v1/cluster/tools", { signal })),
  retry: false,
})

export function refreshClusterTools() {
  void queryClient.invalidateQueries({ queryKey: clusterToolsQuery.queryKey })
}

/** Writes nothing. */
export async function planClusterTools(signal?: AbortSignal) {
  return unwrap(await api.POST("/api/v1/cluster/tools/plan", { signal }))
}

export async function applyClusterTools(token: string) {
  return unwrap(await api.POST("/api/v1/cluster/tools/apply", { body: { token } }))
}

export async function deleteClusterTools(name: string) {
  try {
    unwrap(await api.DELETE("/api/v1/cluster/tools/{name}", { params: { path: { name } } }))
  } catch (error) {
    // Already gone: deleted once its terminals closed, or at its deadline.
    if (!isStatus(error, 404)) throw error
  }
}
