// Your account (`/api/v1/account`) and the members of Zeabur Identity (`/api/v1/members`).
import { queryOptions } from "@tanstack/react-query"

import { type Schemas } from "@/api/client"
import * as zeabur from "@/api/zeabur"
import { queryClient } from "@/api/queries"

export const accountQuery = queryOptions({
  queryKey: ["account"],
  queryFn: async ({ signal }) => await zeabur.getAccount({ signal }),
})

export const membersQuery = queryOptions({
  queryKey: ["members"],
  queryFn: async ({ signal }) => await zeabur.listMembers({ signal }),
})

export function refreshMembers() {
  void queryClient.invalidateQueries({ queryKey: ["members"] })
}

export type SetupLink = Schemas.SetupLinkView

export type MemberChange = Schemas.MemberChange
export type MemberPlan = Schemas.MemberPlan

/** The objects a change to the members writes, and the token that applies them; writes nothing. */
export async function planMemberChange(
  change: MemberChange,
  signal?: AbortSignal,
): Promise<MemberPlan> {
  return await zeabur.planMemberChange(change, { signal })
}

export async function applyMemberChange(token: string): Promise<Schemas.MemberApplied> {
  return await zeabur.applyMemberChange({ token })
}

export async function revokeInvitation(name: string): Promise<void> {
  await zeabur.revokeInvitation(name)
}

export async function createSetupLink(username: string, reset: boolean): Promise<SetupLink> {
  return await zeabur.createSetupLink(username, { reset })
}

export async function removeCredential(username: string, id: string): Promise<void> {
  await zeabur.removeMemberCredential(username, id)
}
