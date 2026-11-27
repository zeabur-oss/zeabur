// The installs this tab runs while their dialog is closed: one line each, which opens the dialog.
import { Link } from "@tanstack/react-router"
import { useEffect } from "react"

import { StatusDot } from "@/components/status"
import { useT } from "@/i18n"
import { type Install, installs, openPlan, progressOf } from "@/lib/installs"
import type { Health } from "@/model/service"

const healthOf = (install: Install): Health =>
  install.running ? "progress" : install.plan?.done ? "ok" : "fail"

/** Mounted once in the shell. Closing or reloading the tab asks first while an install runs. */
export function InstallIndicator() {
  const list = installs.useValue()
  const open = openPlan.useValue()
  const running = list.some((i) => i.running)
  useEffect(() => {
    if (!running) return
    const ask = (event: BeforeUnloadEvent) => event.preventDefault()
    window.addEventListener("beforeunload", ask)
    return () => window.removeEventListener("beforeunload", ask)
  }, [running])
  const shown = list.filter((i) => i.id !== open)
  if (shown.length === 0) return null
  return (
    <ul className="fixed bottom-4 left-1/2 z-40 flex -translate-x-1/2 flex-col gap-2">
      {shown.map((install) => (
        <li key={install.id}>
          <Row install={install} />
        </li>
      ))}
    </ul>
  )
}

function Row({ install }: { install: Install }) {
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
    <Link
      to="/cluster/components"
      search={{ component: install.id }}
      className="flex items-center gap-2 rounded-full border border-main bg-card px-3 py-1.5 text-sm shadow-md hover:border-foreground/20"
    >
      <StatusDot health={healthOf(install)} />
      <span className="font-medium" translate="no">
        {install.name}
      </span>
      <span className="text-muted-foreground" role="status">
        {state}
      </span>
    </Link>
  )
}
