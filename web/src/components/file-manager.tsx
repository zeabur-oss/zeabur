// The parts of a file manager that the container's files and a bucket's files share: the
// dialogs, the table's classes, the write buttons and the upload progress.
import { useRef, useState, type ReactNode } from "react"
import FolderPlusIcon from "~icons/ph/folder-plus"
import UploadSimpleIcon from "~icons/ph/upload-simple"
import WarningFillIcon from "~icons/ph/warning-fill"
import XIcon from "~icons/ph/x"

import { ApiError, localProblem, toProblem, type Problem as ProblemBody } from "@/api/client"
import { REQUEST_HEADERS } from "@/auth/session"
import { Problem } from "@/components/problem"
import { Alert, AlertDescription } from "@zeabur-oss/design-system/components/alert"
import { Button } from "@zeabur-oss/design-system/components/button"
import { Card } from "@zeabur-oss/design-system/components/card"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogTitle,
} from "@zeabur-oss/design-system/components/dialog"
import { Tooltip } from "@zeabur-oss/design-system/components/tooltip"
import { useI18n, useT } from "@/i18n"
import { formatSize } from "@/lib/format"
import { cn } from "@/lib/utils"

export function useSize() {
  const { locale } = useI18n()
  return (bytes: number) => formatSize(bytes, locale)
}

export const head =
  "px-3 py-2 text-left type-meta font-medium text-muted-foreground whitespace-nowrap"
export const cell = "px-3 py-2 align-middle max-sm:p-0"
export const thead = "border-b bg-muted/30 max-sm:hidden"
export const row =
  "max-sm:flex max-sm:flex-wrap max-sm:items-center max-sm:gap-x-3 max-sm:gap-y-1 max-sm:px-4 max-sm:py-3"
export const opener =
  "flex max-w-full min-w-0 items-center gap-2 rounded-sm text-left hover:underline disabled:no-underline"

function WriteButton({
  why,
  label,
  icon,
  primary,
  onClick,
}: {
  why?: string
  label: string
  icon: ReactNode
  primary?: boolean
  onClick: () => void
}) {
  const button = (
    <Button
      variant={primary ? "default" : "outline"}
      size="sm"
      disabled={Boolean(why)}
      onClick={onClick}
    >
      <span aria-hidden="true" data-icon="inline-start" className="contents">
        {icon}
      </span>
      {label}
    </Button>
  )
  return why ? (
    <Tooltip content={why}>
      <span className="inline-flex">{button}</span>
    </Tooltip>
  ) : (
    button
  )
}

export function DialogBar({ title, mono }: { title: ReactNode; mono?: boolean }) {
  const t = useT()
  return (
    <div className="flex items-center gap-2 border-b px-6 py-3">
      <div className="min-w-0 flex-1 truncate">
        <DialogTitle>
          <span
            className={cn("block truncate", mono && "font-mono")}
            translate={mono ? "no" : undefined}
          >
            {title}
          </span>
        </DialogTitle>
      </div>
      <DialogClose
        render={
          <Button variant="ghost" size="icon-sm" className="-mr-1" aria-label={t("common.close")} />
        }
      >
        <XIcon aria-hidden="true" />
      </DialogClose>
    </div>
  )
}

/** A confirmation: what happens, the warning that goes with the mount, and the button that does it. */
export function Confirm({
  title,
  children,
  action,
  destructive,
  busy,
  problem,
  onConfirm,
  onClose,
}: {
  title: ReactNode
  children: ReactNode
  action: string
  destructive?: boolean
  busy: boolean
  problem?: ProblemBody
  onConfirm: () => void
  onClose: () => void
}) {
  const t = useT()
  return (
    <Dialog open onOpenChange={(open) => (open || busy ? undefined : onClose())}>
      <DialogContent variant="compact">
        <form
          className="flex min-h-0 flex-1 flex-col"
          onSubmit={(event) => {
            event.preventDefault()
            onConfirm()
          }}
        >
          <DialogBar title={title} />
          <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto px-6 py-4 text-sm">
            {children}
            {problem ? <Problem problem={problem} /> : null}
          </div>
          <div className="flex items-center justify-end gap-2 border-t px-6 py-3">
            <Button type="button" variant="ghost" size="sm" onClick={onClose} disabled={busy}>
              {t("plan.cancel")}
            </Button>
            <Button
              type="submit"
              variant={destructive ? "destructive" : "default"}
              size="sm"
              disabled={busy}
            >
              {action}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  )
}

export interface Upload {
  name: string
  size: number
  done: number
  state: "waiting" | "sending" | "done" | "failed"
  problem?: ProblemBody
}

export function UploadProgress({
  uploads,
  onDismiss,
}: {
  uploads: Upload[]
  onDismiss: () => void
}) {
  const t = useT()
  const size = useSize()
  const finished = uploads.every((u) => u.state === "done" || u.state === "failed")
  return (
    <Card variant="list" aria-live="polite">
      <div className="flex min-h-11 items-center gap-2 px-4">
        <UploadSimpleIcon aria-hidden="true" className="size-4 text-muted-foreground" />
        <h2 className="min-w-0 flex-1 truncate text-sm font-medium">
          {finished ? t("files.uploadsDone", uploads.length) : t("files.uploading", uploads.length)}
        </h2>
        {finished ? (
          <Button variant="ghost" size="xs" onClick={onDismiss}>
            {t("common.close")}
          </Button>
        ) : null}
      </div>
      <ul className="flex flex-col divide-y">
        {uploads.map((upload) => {
          const share =
            upload.size === 0 ? (upload.state === "done" ? 1 : 0) : upload.done / upload.size
          return (
            <li key={upload.name} className="flex flex-col gap-1.5 px-4 py-2.5">
              <div className="flex items-center gap-3 text-sm">
                <span className="min-w-0 flex-1 truncate font-mono" translate="no">
                  {upload.name}
                </span>
                <span className="type-meta text-muted-foreground tabular-nums">
                  {upload.state === "done"
                    ? t("files.uploaded")
                    : upload.state === "waiting"
                      ? t("files.waiting")
                      : upload.state === "failed"
                        ? t("files.failed")
                        : t("files.of", size(upload.done), size(upload.size))}
                </span>
              </div>
              {upload.problem ? (
                <Problem problem={upload.problem} />
              ) : (
                <div
                  className="h-1 overflow-hidden rounded-full bg-muted"
                  role="progressbar"
                  aria-label={upload.name}
                  aria-valuemin={0}
                  aria-valuemax={100}
                  aria-valuenow={Math.round(share * 100)}
                >
                  <div
                    className={cn(
                      "h-full w-(--share) rounded-full",
                      upload.state === "done" ? "bg-ok" : "bg-progress",
                    )}
                    style={{ "--share": `${share * 100}%` }}
                  />
                </div>
              )}
            </li>
          )
        })}
      </ul>
    </Card>
  )
}

/** PUTs `body` as raw bytes with progress, and resolves to the answer's text; the browser sets
 * `Content-Length`. */
export function putBytes(url: string, body: Blob, onProgress: (done: number) => void) {
  return new Promise<string>((resolve, reject) => {
    const request = new XMLHttpRequest()
    request.open("PUT", url)
    for (const [name, value] of Object.entries(REQUEST_HEADERS))
      request.setRequestHeader(name, value)
    request.upload.onprogress = (event) => onProgress(event.loaded)
    request.onload = () => {
      if (request.status >= 200 && request.status < 300) return resolve(request.responseText)
      let problem: ProblemBody
      try {
        problem = JSON.parse(request.responseText) as ProblemBody
      } catch {
        problem = localProblem(request.status, `The server answered ${request.status}.`)
      }
      reject(new ApiError(request.status, problem))
    }
    request.onerror = () => reject(new ApiError(0, localProblem(0, "The upload was cut off.")))
    request.send(body)
  })
}

/** The table's header: name, size, modified, and the actions column. */
export function FileTableHead() {
  const t = useT()
  return (
    <thead className={thead}>
      <tr>
        <th className={`${head} sm:w-1/2 sm:pl-4`}>{t("files.path")}</th>
        <th className={head}>{t("files.what")}</th>
        <th className={`${head} sm:pr-4 text-right`}>
          <span className="sr-only">{t("files.keeps")}</span>
        </th>
      </tr>
    </thead>
  )
}

/** New folder and Upload, with the file picker Upload opens. */
export function WriteActions({
  why,
  onFolder,
  onFiles,
}: {
  why?: string
  onFolder: () => void
  onFiles: (files: File[]) => void
}) {
  const t = useT()
  const picker = useRef<HTMLInputElement>(null)
  return (
    <>
      <WriteButton
        why={why}
        label={t("files.newFolder")}
        icon={<FolderPlusIcon />}
        onClick={onFolder}
      />
      <WriteButton
        why={why}
        label={t("files.upload")}
        icon={<UploadSimpleIcon />}
        primary
        onClick={() => picker.current?.click()}
      />
      <input
        ref={picker}
        type="file"
        multiple
        hidden
        onChange={(event) => {
          const chosen = [...(event.target.files ?? [])]
          event.target.value = ""
          if (chosen.length > 0) onFiles(chosen)
        }}
      />
    </>
  )
}

/** Runs a write once, then `refresh`; keeps its problem for the dialog to show. */
export function useWrite(refresh: () => Promise<unknown>, onDone: () => void) {
  const [busy, setBusy] = useState(false)
  const [problem, setProblem] = useState<ProblemBody>()
  const run = async (write: () => Promise<unknown>) => {
    setBusy(true)
    setProblem(undefined)
    try {
      await write()
      await refresh()
      onDone()
    } catch (error) {
      setProblem(toProblem(error))
    } finally {
      setBusy(false)
    }
  }
  return { busy, problem, run }
}

/** Sends the files one at a time with `sendOne`, telling `onProgress` how each goes. */
export async function sendAll(
  chosen: File[],
  sendOne: (file: File, onProgress: (done: number) => void) => Promise<unknown>,
  onProgress: (uploads: Upload[]) => void,
) {
  const state: Upload[] = chosen.map((f) => ({
    name: f.name,
    size: f.size,
    done: 0,
    state: "waiting",
  }))
  const update = (index: number, change: Partial<Upload>) => {
    state[index] = { ...state[index]!, ...change }
    onProgress([...state])
  }
  onProgress([...state])
  for (const [index, file] of chosen.entries()) {
    update(index, { state: "sending" })
    try {
      await sendOne(file, (done) => update(index, { done }))
      update(index, { state: "done", done: file.size })
    } catch (error) {
      update(index, { state: "failed", problem: toProblem(error) })
    }
  }
}

/** The files chosen to upload, with the ones that replace a file of the same name marked. */
export function ChosenFiles({ chosen, existing }: { chosen: File[]; existing: Set<string> }) {
  const t = useT()
  const size = useSize()
  const replacing = chosen.filter((f) => existing.has(f.name))
  return (
    <>
      <ul className="flex flex-col divide-y rounded-lg border">
        {chosen.map((file) => (
          <li key={file.name} className="flex items-center gap-3 px-3 py-2">
            <span className="min-w-0 flex-1 truncate font-mono" translate="no">
              {file.name}
            </span>
            {existing.has(file.name) ? (
              <span className="type-meta text-warn-strong">{t("files.replaces")}</span>
            ) : null}
            <span className="type-meta text-muted-foreground tabular-nums">{size(file.size)}</span>
          </li>
        ))}
      </ul>
      {replacing.length > 0 ? (
        <Alert variant="warn" role="note">
          <WarningFillIcon aria-hidden="true" />
          <AlertDescription>{t("files.replaceWarning", replacing.length)}</AlertDescription>
        </Alert>
      ) : null}
    </>
  )
}
