// The cluster's GitHub App, as the web interface shows it: its name on GitHub, where GitHub sends
// the browser back, and the repositories a project may pick from.
import type { Schemas } from "@/api/client"
import { parseRepository } from "@/model/git-service"

/** Cluster → GitHub, where GitHub sends the browser back after making the App. */
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
export const repositoriesOf = (data: Schemas.ProjectGitHubApp | undefined): GitHubRepository[] =>
  (data?.installations ?? [])
    .flatMap((i) => i.repositories.map((r) => ({ installationId: i.id, account: i.account, ...r })))
    .sort((a, b) => a.fullName.localeCompare(b.fullName))

/** The projects to offer for a grant: every project, and what the grant names already. */
export const grantChoices = (projects: readonly string[], granted: readonly string[]): string[] =>
  [...new Set([...projects, ...granted])].sort((a, b) => a.localeCompare(b))

/** What the Git form offers a project that cannot pick a GitHub repository yet. */
export type ConnectStep =
  | { step: "ask" }
  | { step: "create" }
  | { step: "install"; installUrl: string }
  | { step: "grant"; installations: Schemas.GitHubInstallation[] }
  | { step: "problem" }

export function connectStep(
  data: Schemas.GitHubApp | undefined,
  project: string,
): ConnectStep | undefined {
  if (!data) return undefined
  if (data.canChange === "denied") return { step: "ask" }
  if (!data.app)
    return data.unreadable.some((u) => u.resource === "secrets")
      ? { step: "ask" }
      : { step: "create" }
  if (!data.installations) return { step: "problem" }
  // A `*` grant already covers the project; narrowing it to a list would take it from the rest.
  const grantable = data.installations.filter(
    (i) => !i.everyNamespace && !i.namespaces.includes(project),
  )
  if (grantable.length > 0) return { step: "grant", installations: grantable }
  return { step: "install", installUrl: data.app.installUrl }
}

/** The projects of a grant with `project` added. */
export const withProject = (namespaces: readonly string[], project: string): string[] =>
  grantChoices([project], namespaces)

/** The granted repository a typed URL names, which the App can then read. */
export function grantedRepository(
  repositories: readonly GitHubRepository[],
  url: string,
): GitHubRepository | undefined {
  const repo = parseRepository(url)
  if (repo?.host !== "github.com") return undefined
  const name = repo.path.toLowerCase()
  return repositories.find((r) => r.fullName.toLowerCase() === name)
}
