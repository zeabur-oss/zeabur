// The cluster's GitHub App, as the web interface shows it: its name on GitHub, where GitHub sends
// the browser back, and the repositories a project may pick from.
import type { Schemas } from "@/api/client"

/** Where GitHub sends the browser back after making the App. */
export const CALLBACK_PATH = "/cluster/github-app"

/** GitHub allows at most 34 characters. */
const NAME_LIMIT = 34

export const appName = (cluster: string): string =>
  `Zeabur ${cluster.trim()}`.slice(0, NAME_LIMIT).trim()

export function nameProblem(name: string): "empty" | "long" | undefined {
  const value = name.trim()
  if (value === "") return "empty"
  return [...value].length > NAME_LIMIT ? "long" : undefined
}

export function webhookProblem(url: string): "invalid" | undefined {
  const value = url.trim()
  return value === "" || /^https:\/\/[^\s/]+/.test(value) ? undefined : "invalid"
}

export interface GitHubRepository {
  installationId: number
  account: string
  fullName: string
  defaultBranch: string
  private: boolean
}

/** Every repository a project may build, by name. */
export const repositoriesOf = (data: Schemas["ProjectGitHubApp"] | undefined): GitHubRepository[] =>
  (data?.installations ?? [])
    .flatMap((i) => i.repositories.map((r) => ({ installationId: i.id, account: i.account, ...r })))
    .sort((a, b) => a.fullName.localeCompare(b.fullName))

/** The projects to offer for a grant: every project, and what the grant names already. */
export const grantChoices = (projects: readonly string[], granted: readonly string[]): string[] =>
  [...new Set([...projects, ...granted])].sort((a, b) => a.localeCompare(b))
