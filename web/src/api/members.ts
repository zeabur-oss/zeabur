// Your account and the members of Zeabur Identity (`/api/v1/account`, `/api/v1/members`).
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

/** Both answers hold passkeys and links: a change re-reads both. */
export function refreshMembers() {
  void queryClient.invalidateQueries({ queryKey: ["account"] })
  void queryClient.invalidateQueries({ queryKey: ["members"] })
}

export type SetupLink = Schemas["SetupLinkView"]

/** A `204 No Content` carries no data for `unwrap`; anything else is its problem. */
function done(result: { data?: unknown; error?: unknown; response: Response }): void {
  if (!result.response.ok) unwrap(result)
}

export async function createOwnSetupLink(): Promise<SetupLink> {
  return unwrap(await api.POST("/api/v1/account/setup-links"))
}

export async function renamePasskey(id: string, label: string): Promise<void> {
  done(
    await api.PATCH("/api/v1/account/passkeys/{id}", {
      params: { path: { id } },
      body: { label: label.trim() || null },
    }),
  )
}

export async function removeOwnPasskey(id: string): Promise<void> {
  done(await api.DELETE("/api/v1/account/passkeys/{id}", { params: { path: { id } } }))
}

export async function inviteMember(body: Schemas["InviteRequest"]): Promise<SetupLink> {
  return unwrap(await api.POST("/api/v1/members/invitations", { body }))
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

export async function setMemberAccess(
  username: string,
  body: Schemas["AccessRequest"],
): Promise<void> {
  done(await api.PUT("/api/v1/members/{username}/access", { params: { path: { username } }, body }))
}

export async function removeMember(username: string): Promise<void> {
  done(await api.DELETE("/api/v1/members/{username}", { params: { path: { username } } }))
}
