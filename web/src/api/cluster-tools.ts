// The kubectl terminal for the whole cluster (`/api/v1/cluster/tools`), for cluster administrators.
import { queryOptions } from "@tanstack/react-query"

import { isStatus, type Schemas } from "@/api/client"
import * as zeabur from "@/api/zeabur"
import { queryClient } from "@/api/queries"

export type ClusterToolsPlan = Schemas.ClusterToolsPlan

export const clusterToolsQuery = queryOptions({
  queryKey: ["cluster", "tools"],
  queryFn: async ({ signal }) => await zeabur.getClusterTools({ signal }),
  retry: false,
})

export function refreshClusterTools() {
  void queryClient.invalidateQueries({ queryKey: clusterToolsQuery.queryKey })
}

/** Writes nothing. */
export async function planClusterTools(signal?: AbortSignal) {
  return await zeabur.planClusterTools({ signal })
}

export async function applyClusterTools(token: string) {
  return await zeabur.applyClusterTools({ token })
}

export async function deleteClusterTools(name: string) {
  try {
    await zeabur.deleteClusterTools(name)
  } catch (error) {
    // Already gone: deleted once its terminals closed, or at its deadline.
    if (!isStatus(error, 404)) throw error
  }
}
