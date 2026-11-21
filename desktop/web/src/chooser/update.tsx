import { Button } from "@zeabur-oss/design-system/components/button"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogTitle,
} from "@zeabur-oss/design-system/components/dialog"
import { LoadingDots } from "@zeabur-oss/design-system/components/spinner"
import { useSyncExternalStore, type ReactElement } from "react"
import ArrowCircleUpIcon from "~icons/ph/arrow-circle-up"
import CheckIcon from "~icons/ph/check"
import XIcon from "~icons/ph/x"

import { useI18n } from "@/i18n"

import { check, close, install, openReleases, show, snapshot, subscribe } from "./updates"

const useUpdateState = () => useSyncExternalStore(subscribe, snapshot)

/** The version and the update check, at the foot of the sidebar or the first-run page. */
export function UpdateStatus() {
  const { t } = useI18n()
  const { version, status } = useUpdateState()

  let side: ReactElement | null
  switch (status.kind) {
    case "checking":
      side = (
        <span className="flex items-center gap-1.5 type-meta text-tertiary" role="status">
          <LoadingDots />
          {t("desktop.update.checking")}
        </span>
      )
      break
    case "latest":
      side = (
        <span className="flex items-center gap-1 type-meta text-ok-strong" role="status">
          <CheckIcon className="size-3.5" aria-hidden="true" />
          {t("desktop.update.latest")}
        </span>
      )
      break
    case "failed":
      side =
        status.reason === "notConfigured" ? null : (
          <Button variant="ghost" size="xs" className="-mr-2.5" onClick={() => void check()}>
            {t("desktop.update.retry")}
          </Button>
        )
      break
    case "available":
      side = null
      break
    case "idle":
      side = (
        <Button variant="ghost" size="xs" className="-mr-2.5" onClick={() => void check()}>
          {t("desktop.update.check")}
        </Button>
      )
      break
  }

  return (
    <div className="flex flex-col gap-2">
      {status.kind === "available" ? (
        <Button size="sm" className="w-full" onClick={show}>
          <ArrowCircleUpIcon aria-hidden="true" />
          {t("desktop.update.available", status.update.version)}
        </Button>
      ) : null}
      <div className="flex h-control-xs items-center gap-2">
        <span className="min-w-0 flex-1 truncate type-meta text-tertiary tabular-nums">
          {version ? t("desktop.update.version", version) : null}
        </span>
        {side}
      </div>
      {status.kind === "failed" ? (
        <p className="type-meta text-secondary" role="status">
          {status.reason === "notConfigured"
            ? t("desktop.update.notConfigured")
            : t("desktop.update.failed")}
        </p>
      ) : null}
    </div>
  )
}

/** The newer version and its install, opened from UpdateStatus or by a check that found it. */
export function UpdateDialog() {
  const { t } = useI18n()
  const { status, open, installing, installFailed } = useUpdateState()
  if (status.kind !== "available") return null
  const { update } = status

  const actions =
    update.install === "inPlace" ? (
      <>
        <Button variant="secondary" disabled={installing} onClick={close}>
          {t("desktop.update.later")}
        </Button>
        <Button loading={installing} disabled={installing} onClick={install}>
          {installFailed ? t("desktop.update.retry") : t("desktop.update.install")}
        </Button>
      </>
    ) : (
      <>
        <Button variant="secondary" onClick={close}>
          {t("desktop.update.close")}
        </Button>
        <Button onClick={() => void openReleases()}>{t("desktop.update.openReleases")}</Button>
      </>
    )

  return (
    <Dialog open={open} onOpenChange={(next) => (next ? undefined : close())}>
      <DialogContent>
        <div className="flex items-center gap-2 border-b px-5 py-3">
          <div className="min-w-0 flex-1 truncate">
            <DialogTitle>{t("desktop.update.title", update.version)}</DialogTitle>
          </div>
          <DialogClose
            disabled={installing}
            render={
              <Button variant="ghost" size="icon-sm" aria-label={t("desktop.update.close")} />
            }
          >
            <XIcon aria-hidden="true" />
          </DialogClose>
        </div>
        <div className="flex flex-col gap-4 px-5 py-4">
          <p className="type-body text-secondary">{t("desktop.update.current", update.current)}</p>
          {update.notes ? (
            <div className="flex flex-col gap-1.5">
              <p className="type-label text-tertiary">{t("desktop.update.notes")}</p>
              <div className="max-h-64 overflow-y-auto rounded-md border border-v2-border px-3 py-2 type-meta whitespace-pre-wrap text-main select-text">
                {update.notes}
              </div>
            </div>
          ) : null}
          {update.install === "package" ? (
            <p className="type-body text-main">{t("desktop.update.package")}</p>
          ) : null}
          {update.install === "download" ? (
            <p className="type-body text-main">{t("desktop.update.download")}</p>
          ) : null}
          {installFailed ? (
            <p className="type-body text-fail-strong" role="alert">
              {t("desktop.update.installFailed")}
            </p>
          ) : null}
          <div className="flex justify-end gap-2">{actions}</div>
        </div>
      </DialogContent>
    </Dialog>
  )
}
