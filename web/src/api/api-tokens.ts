// A project's API tokens (`/api/v1/projects/{project}/api-tokens`).
import { queryOptions } from "@tanstack/react-query"

import { type Schemas } from "@/api/client"
import { queryClient } from "@/api/queries"
import * as zeabur from "@/api/zeabur"

export const apiTokensQuery = (project: string) =>
  queryOptions({
    queryKey: ["api-tokens", project],
    queryFn: async ({ signal }) => await zeabur.listApiTokens(project, { signal }),
  })

export function refreshApiTokens(project: string) {
  void queryClient.invalidateQueries({ queryKey: ["api-tokens", project] })
}

export type ApiTokenChange = Schemas.ApiTokenChange

export async function planApiTokenChange(
  project: string,
  change: ApiTokenChange,
  signal?: AbortSignal,
): Promise<Schemas.ApiTokenPlan> {
  return await zeabur.planApiTokenChange(project, change, { signal })
}

export async function applyApiTokenChange(
  project: string,
  token: string,
): Promise<Schemas.ApiTokenApplied> {
  return await zeabur.applyApiTokenChange(project, { token })
}
