// Your account (`/api/v1/account`) and the members of Zeabur Identity (`/api/v1/members`).
import { queryOptions } from "@tanstack/react-query"

import { api, unwrap, type Schemas } from "@/api/client"
import { queryClient } from "@/api/queries"

export const accountQuery = queryOptions({
  queryKey: ["account"],
  queryFn: async ({ signal }) => unwrap(await api.GET("/api/v1/account", { signal })),
})

export const membersQuery = queryOptions({
  queryKey: ["members"],
  queryFn: async ({ signal }) => unwrap(await api.GET("/api/v1/members", { signal })),
})

export function refreshMembers() {
  void queryClient.invalidateQueries({ queryKey: ["members"] })
}

export type SetupLink = Schemas["SetupLinkView"]

/** A `204 No Content` carries no data for `unwrap`; anything else is its problem. */
function done(result: { data?: unknown; error?: unknown; response: Response }): void {
  if (!result.response.ok) unwrap(result)
}

export type MemberChange = Schemas["MemberChange"]
export type MemberPlan = Schemas["MemberPlan"]
export type MemberWrite = Schemas["MemberWrite"]

/** The objects a change to the members writes, and the token that applies them; writes nothing. */
export async function planMemberChange(
  change: MemberChange,
  signal?: AbortSignal,
): Promise<MemberPlan> {
  return unwrap(await api.POST("/api/v1/members/plan", { body: change, signal }))
}

export async function applyMemberChange(token: string): Promise<Schemas["MemberApplied"]> {
  return unwrap(await api.POST("/api/v1/members/apply", { body: { token } }))
}

export async function revokeInvitation(name: string): Promise<void> {
  done(await api.DELETE("/api/v1/members/invitations/{name}", { params: { path: { name } } }))
}

export async function createSetupLink(username: string, reset: boolean): Promise<SetupLink> {
  return unwrap(
    await api.POST("/api/v1/members/{username}/setup-links", {
      params: { path: { username } },
      body: { reset },
    }),
  )
}

export async function removeCredential(username: string, id: string): Promise<void> {
  done(
    await api.DELETE("/api/v1/members/{username}/credentials/{id}", {
      params: { path: { username, id } },
    }),
  )
}
