import type { ReactNode } from "react"
import WarningFillIcon from "~icons/ph/warning-fill"
import XIcon from "~icons/ph/x"

import { useT } from "@/i18n"
import { Alert, AlertDescription } from "@zeabur/design-system/components/alert"
import { Button } from "@zeabur/design-system/components/button"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogTitle,
} from "@zeabur/design-system/components/dialog"

/** The frame every dialog here shares with the plan's: title, body, then the buttons. */
export function Frame({
  title,
  wide,
  onClose,
  children,
  footer,
}: {
  title: string
  wide?: boolean
  onClose: () => void
  children: ReactNode
  footer: ReactNode
}) {
  const t = useT()
  return (
    <Dialog open onOpenChange={(open) => (open ? undefined : onClose())}>
      <DialogContent variant={wide ? "wide" : "compact"}>
        <div className="flex items-center gap-2 border-b px-5 py-3">
          <div className="min-w-0 flex-1 truncate">
            <DialogTitle>{title}</DialogTitle>
          </div>
          <DialogClose
            render={<Button variant="ghost" size="icon-sm" aria-label={t("common.close")} />}
          >
            <XIcon aria-hidden="true" />
          </DialogClose>
        </div>
        <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto px-5 py-4 text-sm">
          {children}
        </div>
        <div className="flex flex-wrap items-center justify-end gap-2 border-t px-5 py-3">
          {footer}
        </div>
      </DialogContent>
    </Dialog>
  )
}

/** What will be lost, first, in the plan's warning callout. */
export function Consequence({ children }: { children: ReactNode }) {
  return (
    <Alert variant="warn" role="note">
      <WarningFillIcon aria-hidden="true" />
      <AlertDescription>{children}</AlertDescription>
    </Alert>
  )
}
