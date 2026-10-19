// The shell around every page (agent-playground's frame, the design system's AppFrame): the global
// sidebar on the window ground, folding away from the panel's header or with ⌘\ (remembered), and
// one content panel holding, inside a project, the project's column, then the top bar and the page.
// Below `md` the sidebar and the project's column move into a sheet opened from the top bar. The ⌘K
// palette, and one toaster for the outcomes of writes.
import { Outlet } from "@tanstack/react-router"
import { lazy, Suspense, useEffect } from "react"
import XIcon from "~icons/ph/x"

import { elevation } from "@/auth/elevation"
import { PlanFallback } from "@/components/plan/fallback"
import { CommandPalette } from "@/components/shell/command-palette"
import { ProjectColumn } from "@/components/shell/project-column"
import { Sidebar } from "@/components/shell/sidebar"
import { TopBar } from "@/components/shell/topbar"
import { useT } from "@/i18n"
import { usePlace } from "@/lib/place"
import { mobileNav, setSidebarCollapsed, sidebarCollapsed } from "@/lib/stores"
import { useTheme } from "@/theme"
import { Button } from "@engawahq/design-system/components/button"
import { Dialog, DialogContent, DialogTitle } from "@engawahq/design-system/components/dialog"
import { AppFrame, FramePanel, FrameSidebar } from "@engawahq/design-system/components/frame"
import { TooltipProvider } from "@engawahq/design-system/components/tooltip"

// Toasts report writes, which start from a lazily loaded dialog: the toaster (and Sonner) load after
// the first page rather than with it.
const Toaster = lazy(() =>
  import("@engawahq/design-system/components/sonner").then((m) => ({ default: m.Toaster })),
)
// "Retry as admin" starts from a failure toast, so its dialog loads only then.
const ElevationDialog = lazy(() =>
  import("@/components/elevation-dialog").then((m) => ({ default: m.ElevationDialog })),
)

export function AppShell() {
  const t = useT()
  const place = usePlace()
  const collapsed = sidebarCollapsed.useValue()
  const mobileOpen = mobileNav.useValue()
  const elevating = elevation.useValue()
  const theme = useTheme()

  // ⌘\ (Ctrl+\ elsewhere) folds the sidebar, as in agent-playground.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "\\" || !(event.metaKey || event.ctrlKey) || event.altKey) return
      // Ctrl+\ is SIGQUIT in the terminal, and a key in any field or editor belongs to it.
      const target = event.target instanceof Element ? event.target : null
      if (target?.closest("input, textarea, select, [contenteditable], .xterm, .cm-editor")) return
      event.preventDefault()
      setSidebarCollapsed(!sidebarCollapsed.get())
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [])

  return (
    <TooltipProvider>
      <AppFrame>
        <a
          href="#content"
          className="sr-only z-50 rounded-md bg-background px-3 py-2 text-sm focus:not-sr-only focus:fixed focus:top-2 focus:left-2"
        >
          {t("shell.skip")}
        </a>
        <FrameSidebar collapsed={collapsed}>
          <Sidebar />
        </FrameSidebar>
        <FramePanel className="flex-row">
          {place.project ? (
            <ProjectColumn project={place.project} className="hidden md:flex" />
          ) : null}
          <div className="flex min-w-0 flex-1 flex-col">
            <TopBar />
            <main
              id="content"
              tabIndex={-1}
              className="relative flex min-h-0 flex-1 flex-col overflow-y-auto overscroll-contain outline-none"
            >
              <Outlet />
            </main>
          </div>
        </FramePanel>
      </AppFrame>
      <Dialog open={mobileOpen} onOpenChange={(open) => mobileNav.set(open)}>
        <DialogContent
          variant="sheet"
          className="flex flex-col overflow-y-auto"
          aria-describedby={undefined}
        >
          <DialogTitle className="sr-only">{t("shell.menu")}</DialogTitle>
          <Button
            variant="muted"
            size="icon-sm"
            onClick={() => mobileNav.set(false)}
            className="absolute top-2.5 right-2.5 z-10"
            aria-label={t("common.close")}
          >
            <XIcon aria-hidden="true" />
          </Button>
          <div
            onClickCapture={(event) => {
              if ((event.target as HTMLElement).closest("a")) mobileNav.set(false)
            }}
            className="flex flex-col"
          >
            <Sidebar />
            {place.project ? (
              <ProjectColumn
                project={place.project}
                className="h-auto w-full border-t border-r-0"
              />
            ) : null}
          </div>
        </DialogContent>
      </Dialog>
      <CommandPalette />
      <PlanFallback />
      {elevating.phase === "idle" ? null : (
        <Suspense fallback={null}>
          <ElevationDialog state={elevating} />
        </Suspense>
      )}
      <Suspense fallback={null}>
        <Toaster theme={theme} position="bottom-right" containerAriaLabel={t("toast.region")} />
      </Suspense>
    </TooltipProvider>
  )
}
