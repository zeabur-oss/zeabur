/** The update check: whether the server found a newer release on GitHub. */

import { queryOptions } from "@tanstack/react-query"

import type { Schemas } from "@/api/client"
import * as zeabur from "@/api/zeabur"

/** How often a page left open asks again. */
const CHECK_EVERY = 60 * 60_000

export type Release = Schemas.ZeaburRelease
export type Newer = Schemas.ZeaburNewer

/** Whether a newer Zeabur is out. A page asks the server when it opens, and again every hour
 * while it stays open. The server asks GitHub at most once an hour. */
export const updateQuery = (enabled: boolean) =>
  queryOptions({
    queryKey: ["cluster", "zeabur", "update"],
    queryFn: ({ signal }) => zeabur.getZeaburUpdate({ signal }),
    enabled,
    staleTime: CHECK_EVERY,
    refetchInterval: CHECK_EVERY,
    retry: false,
  })
