import XIcon from "~icons/ph/x"

import { Command } from "@/components/bits"
import { Button } from "@zeabur-oss/design-system/components/button"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogTitle,
} from "@zeabur-oss/design-system/components/dialog"
import { useT } from "@/i18n"
import type { Needed } from "@/model/installer"

/** The `zeaburctl components export` command that prints the manifests of components to install. */
export function InstallerSheet({
  components,
  reason,
  note,
  onClose,
}: {
  components: Needed[]
  /** What needs them, as a sentence. */
  reason?: string
  /** What else to do, which no component gives. */
  note?: string
  onClose: () => void
}) {
  const t = useT()
  const names = t(
    "common.list",
    components.map((c) => c.name),
  )
  return (
    <Dialog open onOpenChange={(open) => (open ? undefined : onClose())}>
      <DialogContent variant="panel">
        <div className="flex items-center gap-2 border-b px-6 py-3">
          <DialogTitle className="min-w-0 flex-1">{t("installer.installTitle", names)}</DialogTitle>
          <DialogClose
            render={
              <Button
                variant="ghost"
                size="icon-sm"
                className="-mr-1"
                aria-label={t("common.close")}
              />
            }
          >
            <XIcon aria-hidden="true" />
          </DialogClose>
        </div>
        <div className="flex min-h-0 flex-1 flex-col gap-5 overflow-y-auto px-6 py-5">
          {reason ? <p className="text-pretty">{reason}</p> : null}
          <section className="flex flex-col gap-2">
            <p className="text-pretty">{t("installer.export")}</p>
            <Command
              text={`zeaburctl components export ${components.map((c) => c.id).join(" ")}`}
            />
          </section>
          {note ? <p className="type-meta text-pretty text-muted-foreground">{note}</p> : null}
        </div>
      </DialogContent>
    </Dialog>
  )
}
