import { Button } from "@zeabur-oss/design-system/components/button"
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@zeabur-oss/design-system/components/dialog"
import { Menu, MenuContent, MenuItem, MenuTrigger } from "@zeabur-oss/design-system/components/menu"
import { Toaster } from "@zeabur-oss/design-system/components/sonner"
import { useEffect, useSyncExternalStore } from "react"
import { toast } from "sonner"
import ArrowClockwiseIcon from "~icons/ph/arrow-clockwise"
import ArrowCircleUpIcon from "~icons/ph/arrow-circle-up"
import ArrowSquareOutIcon from "~icons/ph/arrow-square-out"
import InfoIcon from "~icons/ph/info"

import { useI18n } from "@/i18n"
import { useTheme } from "@/theme"

import { check, close, install, openReleases, show, snapshot, subscribe } from "./updates"

const useUpdateState = () => useSyncExternalStore(subscribe, snapshot)

/** Shown only while a newer version waits, above the foot of the sidebar or the first-run page. */
export function UpdateButton() {
  const { t } = useI18n()
  const { status } = useUpdateState()
  if (status.kind !== "available") return null
  return (
    <Button variant="secondary" size="sm" className="w-full" onClick={show}>
      <ArrowCircleUpIcon className="text-brand" aria-hidden="true" />
      {t("desktop.update.available", status.update.version)}
    </Button>
  )
}

/** The version and the manual check, beside the language picker. */
export function AboutMenu() {
  const { t } = useI18n()
  const { version, status } = useUpdateState()
  return (
    <Menu>
      <MenuTrigger
        render={<Button variant="muted" size="icon-sm" aria-label={t("desktop.update.about")} />}
      >
        <InfoIcon aria-hidden="true" />
      </MenuTrigger>
      <MenuContent align="start">
        {version ? (
          <p className="px-2.5 pt-1.5 pb-1 type-meta text-tertiary">
            {t("desktop.update.version", version)}
          </p>
        ) : null}
        <MenuItem disabled={status.kind === "checking"} onClick={() => void check()}>
          <ArrowClockwiseIcon aria-hidden="true" />
          {t("desktop.update.check")}
        </MenuItem>
      </MenuContent>
    </Menu>
  )
}

const toastId = "update-check"

/** The answers of a check as toasts; a newer version opens UpdateDialog instead. */
export function UpdateToasts() {
  const { t } = useI18n()
  const theme = useTheme()
  const { status, version } = useUpdateState()

  useEffect(() => {
    switch (status.kind) {
      case "checking":
        toast.loading(t("desktop.update.checking"), { id: toastId })
        break
      case "latest":
        toast.success(t("desktop.update.latestTitle"), {
          id: toastId,
          description: version ? t("desktop.update.latest", version) : undefined,
        })
        break
      case "failed":
        if (status.reason === "notConfigured")
          toast.info(t("desktop.update.notConfigured"), { id: toastId })
        else
          toast.error(t("desktop.update.failedTitle"), {
            id: toastId,
            description: t("desktop.update.failed"),
            action: { label: t("desktop.update.retry"), onClick: () => void check() },
          })
        break
      case "available":
        toast.dismiss(toastId)
        break
      case "idle":
        break
    }
    // A toast is the answer to one check, so a new language does not show it again.
  }, [status])

  return <Toaster theme={theme} position="bottom-right" />
}

/** The newer version and its install, opened from UpdateButton or by a check that found it. */
export function UpdateDialog() {
  const { t } = useI18n()
  const { status, open, installing, installFailed } = useUpdateState()
  if (status.kind !== "available") return null
  const { update } = status

  return (
    <Dialog open={open} onOpenChange={(next) => (next ? undefined : close())}>
      <DialogContent>
        <DialogHeader close={!installing} closeLabel={t("desktop.update.close")}>
          <DialogTitle>{t("desktop.update.title", update.version)}</DialogTitle>
          <DialogDescription>{t("desktop.update.current", update.current)}</DialogDescription>
        </DialogHeader>
        <DialogBody>
          <div className="flex flex-col gap-3 type-body">
            {update.install === "package" ? (
              <p className="text-pretty">{t("desktop.update.package")}</p>
            ) : null}
            {update.install === "download" ? (
              <p className="text-pretty">{t("desktop.update.download")}</p>
            ) : null}
            <Button variant="link" className="w-fit" onClick={() => void openReleases()}>
              {t("desktop.update.whatsNew", update.version)}
              <ArrowSquareOutIcon className="size-3" aria-hidden="true" />
            </Button>
            {installFailed ? (
              <p className="text-fail-strong" role="alert">
                {t("desktop.update.installFailed")}
              </p>
            ) : null}
          </div>
        </DialogBody>
        <DialogFooter>
          {update.install === "inPlace" ? (
            <>
              <Button variant="ghost" size="sm" disabled={installing} onClick={close}>
                {t("desktop.update.later")}
              </Button>
              <Button size="sm" loading={installing} disabled={installing} onClick={install}>
                {installFailed ? t("desktop.update.retry") : t("desktop.update.install")}
              </Button>
            </>
          ) : (
            <>
              <Button variant="ghost" size="sm" onClick={close}>
                {t("desktop.update.close")}
              </Button>
              <Button size="sm" onClick={() => void openReleases()}>
                {t("desktop.update.openReleases")}
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
