import type { Page } from "@/api/pages"

export type { Page }

/** The pages under the Cluster area. */
export const clusterPages: Page[] = [
  "overview",
  "backups",
  "notifications",
  "githubApp",
  "components",
]

const pagesByRoute: Record<string, Page[]> = {
  "/cluster": clusterPages,
  "/cluster/backups": ["backups"],
  "/cluster/notifications": ["notifications"],
  "/cluster/components": ["components"],
  "/cluster/github-app": ["githubApp"],
  "/cluster/github-app/continue": ["githubApp"],
  "/members": ["members"],
}

/** The pages any of which opens `route`; none for a route that is not only for administrators. */
export const pagesOfRoute = (route: string): Page[] | undefined => pagesByRoute[route]

/** An answer that is not yet known, or failed, counts as no. */
export type Answers = Partial<Record<Page, boolean>>

/** Whether any of `pages` is open. */
export const anyOpen = (answers: Answers, pages: Page[]): boolean =>
  pages.some((page) => answers[page] === true)
