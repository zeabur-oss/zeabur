import { useState } from "react"

import { FolderDrop, useFolder } from "@/components/add/folder-drop"
import { FormDrawer } from "@/components/plan/plan-drawer"
import { useI18n, useT } from "@/i18n"
import { formatSize } from "@/lib/format"
import { retryUpload, startUpload, useUpload } from "@/model/upload"
import { Alert, AlertAction, AlertDescription } from "@zeabur-oss/design-system/components/alert"
import { Button } from "@zeabur-oss/design-system/components/button"
import { Meter } from "@zeabur-oss/design-system/components/meter"

/** Where an upload to the Build is: asking for permission, sending, starting the build, or why
 * it stopped. */
export function UploadProgress({ project, build }: { project: string; build: string }) {
  const t = useT()
  const { locale } = useI18n()
  const state = useUpload(project, build)
  if (!state || state.phase === "done") return null
  if (state.phase === "failed")
    return (
      <Alert variant="destructive" className="mt-2 max-w-prose">
        <AlertDescription>
          <span className="block font-medium">{t(`upload.failed.${state.stage}`)}</span>
          <span className="block" lang="en" translate="no">
            {state.problem.detail}
          </span>
        </AlertDescription>
        <AlertAction>
          <Button variant="outline" size="xs" onClick={() => retryUpload(project, build, t)}>
            {t("upload.retry")}
          </Button>
        </AlertAction>
      </Alert>
    )
  const share = state.total > 0 ? state.sent / state.total : 0
  return (
    <div role="status" className="mt-2 flex max-w-prose flex-col gap-1.5">
      <p className="text-sm text-secondary">
        {state.stage === "token"
          ? t("upload.stage.token")
          : state.stage === "sending"
            ? t(
                "upload.stage.sending",
                formatSize(state.sent, locale),
                formatSize(state.total, locale),
              )
            : t("upload.stage.naming")}
      </p>
      <Meter value={share} tone="ok" size="sm" />
      <p className="type-meta text-muted-foreground">{t("upload.keepOpen")}</p>
    </div>
  )
}

/** Picks a folder for a service that already exists, and sends it to the Build. */
export function UploadFolderDrawer({
  project,
  build,
  onClose,
}: {
  project: string
  build: string
  onClose: () => void
}) {
  const t = useT()
  const folder = useFolder()
  const [tried, setTried] = useState(false)
  const ready = folder.state.phase === "ready" ? folder.state : undefined
  const upload = () => {
    setTried(true)
    if (!ready) return
    void startUpload({ project, build, blob: ready.packed.blob, t })
    onClose()
  }
  return (
    <FormDrawer
      onClose={onClose}
      title={t("builds.upload")}
      description={t("builds.uploadBody", build)}
      footer={
        <div className="ml-auto flex gap-2">
          <Button variant="ghost" size="sm" onClick={onClose}>
            {t("edit.cancel")}
          </Button>
          <Button size="sm" onClick={upload}>
            {t("upload.start")}
          </Button>
        </div>
      }
    >
      <div className="flex flex-col gap-2">
        <FolderDrop folder={folder} missing={tried && !ready && folder.state.phase !== "failed"} />
        {tried && folder.state.phase === "empty" ? (
          <p className="text-xs leading-4 text-destructive">{t("upload.problem.folder")}</p>
        ) : null}
      </div>
    </FormDrawer>
  )
}
