// Whether the person may open an administrator page: a HEAD request on the page's own endpoint.
import { queryOptions } from "@tanstack/react-query"

import { ApiError } from "@/api/client"
import * as zeabur from "@/api/zeabur"

/** The administrator pages. Each is opened by the server's answer to a HEAD request on the page's
 * own endpoint (RFC 9110 section 9.3.2): 200 when the person may read any of what the page lists. */
export type Page = "overview" | "backups" | "notifications" | "githubApp" | "components" | "members"

const heads: Record<Page, () => Promise<void>> = {
  overview: () => zeabur.headClusterNodes(),
  backups: () => zeabur.headClusterBackups(),
  notifications: () => zeabur.headClusterNotifications(),
  githubApp: () => zeabur.headGitHubApp(),
  components: () => zeabur.headCapabilities(),
  members: () => zeabur.headMembers(),
}

/** Yes on 200; a refusal or an unchecked answer is no, and anything else is an error. */
export const pageQuery = (page: Page) =>
  queryOptions({
    queryKey: ["page-open", page],
    queryFn: async () => {
      try {
        await heads[page]()
        return true
      } catch (error) {
        if (error instanceof ApiError) return false
        throw error
      }
    },
    staleTime: 60_000,
  })
