import { Button } from "@zeabur-oss/design-system/components/button"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogTitle,
} from "@zeabur-oss/design-system/components/dialog"
import { useSyncExternalStore, type ReactElement } from "react"
import XIcon from "~icons/ph/x"

import { useI18n } from "@/i18n"

import { check, close, install, openReleases, show, snapshot, subscribe } from "./updates"

/**
 * The update check, kept in ./updates: an answer when the person asks, from the button or the
 * macOS menu. Installing waits for the person.
 */
export function useUpdates(): { button: ReactElement; dialog: ReactElement } {
  const { t } = useI18n()
  const { available, view, checking } = useSyncExternalStore(subscribe, snapshot)

  const button = available ? (
    <Button size="sm" className="w-full" onClick={() => show(available)}>
      {t("desktop.update.available", available.version)}
    </Button>
  ) : (
    <Button
      variant="ghost"
      size="sm"
      className="w-full"
      disabled={checking}
      onClick={() => void check()}
    >
      {checking ? t("desktop.update.checking") : t("desktop.update.check")}
    </Button>
  )

  let title = ""
  let body: ReactElement | null = null
  let actions: ReactElement | null = null
  if (view?.kind === "available") {
    const { update } = view
    title = t("desktop.update.title", update.version)
    body = (
      <>
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
        {view.error ? (
          <div className="flex flex-col gap-1">
            <p className="type-body text-fail-strong">{t("desktop.update.installFailed")}</p>
            <p className="type-meta break-words text-tertiary select-text">{view.error}</p>
          </div>
        ) : null}
      </>
    )
    actions =
      update.install === "inPlace" ? (
        <>
          <Button variant="secondary" disabled={view.installing} onClick={close}>
            {t("desktop.update.later")}
          </Button>
          <Button disabled={view.installing} onClick={() => install(update)}>
            {view.installing ? t("desktop.update.installing") : t("desktop.update.install")}
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
  } else if (view?.kind === "latest") {
    title = t("desktop.update.latestTitle")
    body = <p className="type-body text-secondary">{t("desktop.update.latest", view.version)}</p>
  } else if (view?.kind === "failed") {
    title = t("desktop.update.failedTitle")
    body =
      view.reason === "not-configured" ? (
        <p className="type-body text-secondary">{t("desktop.update.notConfigured")}</p>
      ) : (
        <p className="type-meta break-words text-tertiary select-text">{view.reason}</p>
      )
  }
  actions ??= <Button onClick={close}>{t("desktop.update.close")}</Button>

  const installing = view?.kind === "available" && view.installing
  const dialog = (
    <Dialog
      open={view !== null}
      onOpenChange={(next) => (next || installing ? undefined : close())}
    >
      <DialogContent>
        <div className="flex items-center gap-2 border-b px-5 py-3">
          <div className="min-w-0 flex-1 truncate">
            <DialogTitle>{title}</DialogTitle>
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
          {body}
          <div className="flex justify-end gap-2">{actions}</div>
        </div>
      </DialogContent>
    </Dialog>
  )
  return { button, dialog }
}
