import { useQuery } from "@tanstack/react-query"
import type { ReactNode } from "react"
import XIcon from "~icons/ph/x"

import { capabilitiesQuery } from "@/api/queries"
import { CopyButton } from "@/components/bits"
import { Button } from "@zeabur/design-system/components/button"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogTitle,
} from "@zeabur/design-system/components/dialog"
import { useT } from "@/i18n"
import type { Needed } from "@/model/installer"

const CONFIG = "/etc/zeabur-installer/config.yaml"

/** The installer commands that install components on the server, all in one step, or remove one.
 * Where zeabur-installer did not set up the cluster, `zeaburctl components export` instead. */
export function InstallerSheet({
  components,
  remove = false,
  reason,
  note,
  onClose,
}: {
  components: Needed[]
  /** Remove the one component instead. */
  remove?: boolean
  /** What needs them, as a sentence. */
  reason?: string
  /** What else to do, which no component gives. */
  note?: string
  onClose: () => void
}) {
  const t = useT()
  // `undefined` while unknown or unreadable: both ways are shown.
  const installer = useQuery({ ...capabilitiesQuery, retry: false }).data?.installer ?? undefined
  const names = t(
    "common.list",
    components.map((c) => c.name),
  )
  return (
    <Dialog open onOpenChange={(open) => (open ? undefined : onClose())}>
      <DialogContent variant="panel">
        <div className="flex items-center gap-2 border-b px-6 py-3">
          <DialogTitle className="min-w-0 flex-1">
            {remove ? t("installer.removeTitle", names) : t("installer.installTitle", names)}
          </DialogTitle>
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
          {installer === false ? null : (
            <InstallerSteps components={components} names={names} remove={remove} />
          )}
          {note ? <p className="type-meta text-pretty text-muted-foreground">{note}</p> : null}
          {remove ? (
            <div className="flex flex-col gap-1 type-meta text-pretty text-muted-foreground">
              {installer === false ? null : <p>{t("installer.stays")}</p>}
              <p>{t("installer.elsewhere")}</p>
            </div>
          ) : installer === true ? null : (
            <ExportStep components={components} installer={installer} />
          )}
        </div>
      </DialogContent>
    </Dialog>
  )
}

function InstallerSteps({
  components,
  names,
  remove,
}: {
  components: Needed[]
  names: string
  remove: boolean
}) {
  const t = useT()
  const many = components.length > 1
  return (
    <>
      <p className="text-pretty">{t("installer.intro")}</p>
      <Step text={remove ? t("installer.guiRemove", names) : t("installer.gui", names)}>
        <Command text="sudo zeabur-installer gui" />
        <p className="type-meta text-pretty text-muted-foreground">{t("installer.guiReach")}</p>
      </Step>
      <Step
        text={remove ? t("installer.configRemove", CONFIG) : t("installer.config", CONFIG, many)}
      >
        {remove ? null : (
          <Command text={`enable:\n${components.map((c) => `  - ${c.id}`).join("\n")}`} />
        )}
        <Command text="sudo zeabur-installer apply" />
      </Step>
    </>
  )
}

function ExportStep({ components, installer }: { components: Needed[]; installer?: boolean }) {
  const t = useT()
  return (
    <section className="flex flex-col gap-2">
      <p className="text-pretty">
        {installer === false ? t("installer.notHere") : t("installer.otherCluster")}
      </p>
      <Command text={`zeaburctl components export ${components.map((c) => c.id).join(" ")}`} />
    </section>
  )
}

function Step({ text, children }: { text: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-2">
      <p className="text-pretty">{text}</p>
      {children}
    </section>
  )
}

function Command({ text }: { text: string }) {
  return (
    <div className="relative min-w-0 rounded-lg border bg-muted/30">
      <CopyButton value={text} className="absolute top-2 right-2" />
      <pre
        className="py-3 pr-12 pl-3 font-mono text-xs leading-5 wrap-anywhere whitespace-pre-wrap"
        translate="no"
      >
        {text}
      </pre>
    </div>
  )
}
