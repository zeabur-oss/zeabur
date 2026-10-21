import { lazy, Suspense, useEffect, useState } from "react"
import TerminalWindowIcon from "~icons/ph/terminal-window"

import { useT } from "@/i18n"
import { dock, dockShortcut, isDockShortcut, toggleDock } from "@/lib/dock"
import { mobileNav } from "@/lib/stores"
import { Kbd } from "@zeabur/design-system/components/kbd"
import { SidebarItem } from "@zeabur/design-system/components/sidebar"

const load = () => import("@/components/dock/terminal-dock")
// Lazy: xterm.js stays out of the first page load (scripts/ui-budget.sh).
const TerminalDock = lazy(() => load().then((m) => ({ default: m.TerminalDock })))

const preloadDock = () => void load()

/** The dock under the page, from the first time it opens; Ctrl+` from anywhere. */
export function DockSlot() {
  const state = dock.useValue()
  const [wanted, setWanted] = useState(false)
  if (!wanted && (state.open || state.tabs.length > 0)) setWanted(true)

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (!isDockShortcut(event)) return
      // Before xterm.js, which takes the keys typed into it.
      event.preventDefault()
      event.stopPropagation()
      toggleDock()
    }
    window.addEventListener("keydown", onKey, true)
    return () => window.removeEventListener("keydown", onKey, true)
  }, [])

  return wanted ? (
    <Suspense fallback={null}>
      <TerminalDock />
    </Suspense>
  ) : null
}

export function DockToggle() {
  const t = useT()
  const { open, tabs } = dock.useValue()
  return (
    <SidebarItem
      icon={<TerminalWindowIcon aria-hidden="true" />}
      trailing={
        <>
          {tabs.length > 0 ? <span>{tabs.length}</span> : null}
          <Kbd keys={dockShortcut} aria-hidden="true" className="max-md:hidden" />
        </>
      }
      aria-expanded={open}
      aria-controls={open || tabs.length > 0 ? "dock" : undefined}
      aria-keyshortcuts="Control+`"
      onPointerEnter={preloadDock}
      onFocus={preloadDock}
      onClick={() => {
        mobileNav.set(false)
        toggleDock()
      }}
    >
      {t("dock.label")}
    </SidebarItem>
  )
}
