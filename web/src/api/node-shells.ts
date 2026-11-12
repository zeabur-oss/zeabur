// Root shells on nodes (`/api/v1/cluster/node-shells`), for cluster administrators.
import { queryOptions } from "@tanstack/react-query"

import { type Schemas } from "@/api/client"
import * as zeabur from "@/api/zeabur"
import { refresh, reviewOnce } from "@/api/queries"

export type NodeShell = Schemas.NodeShellView
export type NodeShellPlan = Schemas.NodeShellPlan
export type NodeShellRequest = Schemas.NodeShellPlanRequest

export const nodeShellsQuery = queryOptions({
  queryKey: ["cluster", "node-shells"],
  queryFn: async ({ signal }) => await zeabur.getNodeShells({ signal }),
  refetchInterval: (query) =>
    query.state.data?.shells.some((shell) => shell.phase === "starting") ? 1500 : 15_000,
  refetchIntervalInBackground: false,
  retry: false,
})

export function refreshNodeShells() {
  void refresh(nodeShellsQuery)
}

/** Writes nothing. */
export const nodeShellPlanQuery = (request: NodeShellRequest) =>
  queryOptions({
    queryKey: ["node-shell-plan", request],
    queryFn: async ({ signal }) => await zeabur.planNodeShell(request, { signal }),
    ...reviewOnce,
  })

export async function applyNodeShell(token: string) {
  return await zeabur.applyNodeShell({ token })
}
