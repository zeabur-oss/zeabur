// Uploading a folder to a Build that builds one: ask for an upload token (a plan), wait for Zeabur
// Builder's answer, send the packed folder to Zeabur, which pushes it to the registry, then name
// its digest in the Build (a plan). The folder stays in memory until the upload is done, so a page
// that was left open can try again; nothing is kept once it is closed.
import { toast } from "sonner"

import { ApiError, localProblem, toProblem, type Problem, type Schemas } from "@/api/client"
import * as zeabur from "@/api/zeabur"
import { buildsQuery, queryClient } from "@/api/queries"
import { sendSource } from "@/api/source"
import type { Translate } from "@/i18n"
import { store } from "@/lib/stores"
import type { Build } from "@/model/builds"

export type UploadStage = "token" | "sending" | "naming"

export type UploadState =
  | { phase: "running"; stage: UploadStage; sent: number; total: number }
  | { phase: "failed"; stage: UploadStage; problem: Problem }
  | { phase: "done" }

interface Entry {
  state: UploadState
  /** Kept while the upload can still be tried again. */
  blob?: Blob
}

const uploads = store<Record<string, Entry>>({})

const keyOf = (project: string, build: string) => `${project}/${build}`

const change = (key: string, state: UploadState, blob?: Blob) =>
  uploads.set({ ...uploads.get(), [key]: { state, blob } })

/** The upload under way or last tried for the Build. */
export const useUpload = (project: string, build: string): UploadState | undefined =>
  uploads.useValue()[keyOf(project, build)]?.state

/** A value Zeabur Builder has not been asked with: at most 63 letters and digits. */
export function newRequest(now = Date.now(), random = Math.random): string {
  return `u${now.toString(36)}${Math.floor(random() * 36 ** 6)
    .toString(36)
    .padStart(6, "0")}`
}

const TOKEN_WAIT_MS = 90_000
const POLL_MS = 1_000

/**
 * Waits until the Build has answered `request`, or `timeout` ms pass. `read` gives the Build as it
 * is now.
 */
export async function waitForToken(
  read: () => Promise<Build | undefined>,
  request: string,
  {
    timeout = TOKEN_WAIT_MS,
    interval = POLL_MS,
    sleep = (ms: number) => new Promise((r) => setTimeout(r, ms)),
  } = {},
): Promise<boolean> {
  for (let waited = 0; waited <= timeout; waited += interval) {
    if ((await read())?.uploadRequest === request) return true
    await sleep(interval)
  }
  return false
}

async function planAndApply(project: string, changes: Schemas.Change[]) {
  const plan = await zeabur.planProject(project, { changes })
  await zeabur.applyProject(project, { token: plan.token })
}

export interface UploadStart {
  project: string
  build: string
  blob: Blob
  /** The token request the Build was created with; left out: ask for a token first. */
  request?: string
  t: Translate
}

/** Uploads `blob` to `build`; the state is in [`uploads`]. */
export async function startUpload({ project, build, blob, request: asked, t }: UploadStart) {
  const key = keyOf(project, build)
  const total = blob.size
  let stage: UploadStage = "token"
  const running = (sent = 0) => change(key, { phase: "running", stage, sent, total }, blob)
  const request = asked ?? newRequest()
  try {
    running()
    if (!asked) await planAndApply(project, [{ op: "uploadToken", build, request }])
    const answered = await waitForToken(async () => {
      const found = await zeabur.getProjectBuilds(project)
      return found.builds.find((b) => b.name === build)
    }, request)
    if (!answered)
      throw new ApiError(
        503,
        localProblem(503, "Zeabur Builder did not answer the request for an upload token."),
      )
    stage = "sending"
    running()
    const { digest } = await sendSource(project, build, request, blob, (sent) => running(sent))
    stage = "naming"
    running(total)
    await planAndApply(project, [{ op: "archiveDigest", build, digest }])
    change(key, { phase: "done" })
    void queryClient.invalidateQueries({ queryKey: buildsQuery(project).queryKey })
    void queryClient.invalidateQueries({ queryKey: ["project", project] })
    toast.success(t("upload.sent", build))
  } catch (error) {
    change(key, { phase: "failed", stage, problem: toProblem(error) }, blob)
  }
}

/** Tries a failed upload again with the folder still in memory. */
export function retryUpload(project: string, build: string, t: Translate) {
  const blob = uploads.get()[keyOf(project, build)]?.blob
  if (blob) void startUpload({ project, build, blob, t })
}
