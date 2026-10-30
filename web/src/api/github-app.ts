// The cluster's GitHub App (`/api/v1/cluster/github-app`) and the installations granted to a
// project (`/api/v1/projects/{project}/github-app`).
import { queryOptions } from "@tanstack/react-query"

import { type Schemas } from "@/api/client"
import * as zeabur from "@/api/zeabur"
import { queryClient } from "@/api/queries"

export type GitHubAppPlan = Schemas.GitHubAppPlan

export const gitHubAppQuery = queryOptions({
  queryKey: ["cluster", "github-app"],
  queryFn: async ({ signal }) => await zeabur.getGitHubApp({ signal }),
  retryOnMount: false,
})

export const projectGitHubAppQuery = (project: string) =>
  queryOptions({
    queryKey: ["projects", project, "github-app"],
    queryFn: async ({ signal }) => await zeabur.getProjectGitHubApp(project, { signal }),
  })

export function refreshGitHubApp() {
  void queryClient.invalidateQueries({ queryKey: ["cluster", "github-app"] })
  void queryClient.invalidateQueries({
    predicate: (q) => q.queryKey[0] === "projects" && q.queryKey[2] === "github-app",
  })
}

/** Starts GitHub's manifest flow; the page to open sends the manifest to GitHub. */
export async function startManifest(body: Schemas.GitHubManifestRequest): Promise<string> {
  return (await zeabur.startGitHubAppManifest(body)).formUrl
}

/** Turns the code GitHub sent back into the App, and plans storing it; writes nothing. */
export async function convertCode(
  body: Schemas.GitHubConversionRequest,
  signal?: AbortSignal,
): Promise<GitHubAppPlan> {
  return await zeabur.convertGitHubApp(body, { signal })
}

/** What granting an installation to projects writes; writes nothing. */
export async function planGrant(
  body: Schemas.GitHubGrantRequest,
  signal?: AbortSignal,
): Promise<GitHubAppPlan> {
  return await zeabur.planGitHubApp(body, { signal })
}

export async function applyGitHubApp(token: string): Promise<Schemas.GitHubAppApplied> {
  return await zeabur.applyGitHubApp({ token })
}
