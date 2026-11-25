// Whether the person may open an administrator page: a HEAD request on the page's own endpoint.
import { queryOptions } from "@tanstack/react-query"

import { ApiError } from "@/api/client"
import * as zeabur from "@/api/zeabur"

/** The administrator pages. Each is opened by the server's answer to a HEAD request on the page's
 * own endpoint (RFC 9110 section 9.3.2): 200 when the person may read any of what the page lists. */
export type Page = "overview" | "backups" | "notifications" | "githubApp" | "components" | "members"

const heads: Record<Page, () => Promise<void>> = {
  overview: () => zeabur.checkClusterNodes(),
  backups: () => zeabur.checkClusterBackups(),
  notifications: () => zeabur.checkClusterNotifications(),
  githubApp: () => zeabur.checkGitHubApp(),
  components: () => zeabur.checkCapabilities(),
  members: () => zeabur.checkMembers(),
}

/** How long to wait before asking again after the server could not answer. */
export const ASK_AGAIN = 30_000

/** Yes on 200 and no on 403. Any other answer, such as a 503 when the server could not check, is
 * an error: it is retried like other reads, counts as no meanwhile, and is asked again later. */
export const pageQuery = (page: Page) =>
  queryOptions({
    queryKey: ["page-open", page],
    queryFn: async () => {
      try {
        await heads[page]()
        return true
      } catch (error) {
        if (error instanceof ApiError && error.status === 403) return false
        throw error
      }
    },
    staleTime: 60_000,
    refetchInterval: (query) => (query.state.status === "error" ? ASK_AGAIN : false),
  })
