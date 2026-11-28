// The installs this tab runs while their dialog is closed: one line each, which opens the dialog.
import { StatusDot } from "@/components/status"
import { useT } from "@/i18n"
import { type Install, installs, progressOf } from "@/lib/installs"
import type { Health } from "@/model/service"

const healthOf = (install: Install): Health =>
  install.running ? "progress" : install.plan?.done ? "ok" : "fail"

/** `opened`: the component whose dialog is open, which shows its install itself. */
export function InstallIndicator({
  opened,
  onOpen,
}: {
  opened: string | undefined
  onOpen: (id: string) => void
}) {
  const shown = installs.useValue().filter((i) => i.id !== opened)
  if (shown.length === 0) return null
  return (
    <ul className="fixed bottom-4 left-1/2 z-40 flex -translate-x-1/2 flex-col gap-2">
      {shown.map((install) => (
        <li key={install.id}>
          <Row install={install} onOpen={() => onOpen(install.id)} />
        </li>
      ))}
    </ul>
  )
}

function Row({ install, onOpen }: { install: Install; onOpen: () => void }) {
  const t = useT()
  const { done, total } = progressOf(install)
  const state = install.running
    ? total > 0
      ? t("components.progress", Math.min(done + 1, total), total)
      : t(install.action === "install" ? "components.installing" : "components.removing")
    : install.plan?.done
      ? t(install.action === "install" ? "components.installDone" : "components.removed")
      : t("components.installStopped")
  return (
    <button
      type="button"
      onClick={onOpen}
      className="flex items-center gap-2 rounded-full border border-main bg-card px-3 py-1.5 text-sm shadow-md hover:border-foreground/20"
    >
      <StatusDot health={healthOf(install)} />
      <span className="font-medium" translate="no">
        {install.name}
      </span>
      <span className="text-muted-foreground" role="status">
        {state}
      </span>
    </button>
  )
}
