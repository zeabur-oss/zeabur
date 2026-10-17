// The shell around every page: the global sidebar (full or an icon rail, remembered), the service
// column inside a project, the top bar and the ⌘K palette. Below `md` both columns move into a
// sheet opened from the top bar. One toaster for the outcomes of writes.
import { Outlet } from "@tanstack/react-router"
import { lazy, Suspense } from "react"
import XIcon from "~icons/ph/x"

import { elevation } from "@/auth/elevation"
import { PlanFallback } from "@/components/plan/fallback"
import { CommandPalette } from "@/components/shell/command-palette"
import { ProjectColumn } from "@/components/shell/project-column"
import { Sidebar } from "@/components/shell/sidebar"
import { TopBar } from "@/components/shell/topbar"
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog"
import { TooltipProvider } from "@/components/ui/tooltip"
import { useT } from "@/i18n"
import { usePlace } from "@/lib/place"
import { mobileNav, projectSidebarRail, setSidebarRail, sidebarRail } from "@/lib/stores"

// Toasts report writes, which start from a lazily loaded dialog: the toaster (and Sonner) load after
// the first page rather than with it.
const Toaster = lazy(() => import("@/components/ui/sonner").then((m) => ({ default: m.Toaster })))
// "Retry as admin" starts from a failure toast, so its dialog loads only then.
const ElevationDialog = lazy(() =>
  import("@/components/elevation-dialog").then((m) => ({ default: m.ElevationDialog })),
)

export function AppShell() {
  const t = useT()
  const place = usePlace()
  const inProject = !!place.project
  const railOutside = sidebarRail.useValue()
  const railInside = projectSidebarRail.useValue()
  const rail = inProject ? railInside : railOutside
  const mobileOpen = mobileNav.useValue()
  const elevating = elevation.useValue()

  return (
    <TooltipProvider>
      <div className="flex h-svh overflow-hidden bg-background text-foreground">
        <a
          href="#content"
          className="sr-only z-50 rounded-lg bg-background px-3 py-2 text-sm focus:not-sr-only focus:fixed focus:top-2 focus:left-2 focus:ring-3 focus:ring-ring"
        >
          {t("shell.skip")}
        </a>
        <div className="hidden md:flex">
          <Sidebar rail={rail} onToggle={() => setSidebarRail(inProject, !rail)} />
          {place.project ? <ProjectColumn project={place.project} /> : null}
        </div>
        <div className="flex min-w-0 flex-1 flex-col">
          <TopBar />
          <main
            id="content"
            tabIndex={-1}
            className="relative flex min-h-0 flex-1 flex-col overflow-y-auto outline-none"
          >
            <Outlet />
          </main>
        </div>
      </div>
      <Dialog open={mobileOpen} onOpenChange={(open) => mobileNav.set(open)}>
        <DialogContent
          variant="sheet"
          className="flex flex-col overflow-y-auto"
          aria-describedby={undefined}
        >
          <DialogTitle className="sr-only">{t("shell.menu")}</DialogTitle>
          <button
            type="button"
            onClick={() => mobileNav.set(false)}
            className="absolute top-3 right-3 z-10 flex size-8 items-center justify-center rounded-lg text-muted-foreground hover:bg-muted"
            aria-label={t("common.close")}
          >
            <XIcon className="size-4" aria-hidden="true" />
          </button>
          <div
            onClickCapture={(event) => {
              if ((event.target as HTMLElement).closest("a")) mobileNav.set(false)
            }}
            className="flex flex-col"
          >
            <Sidebar rail={false} mobile />
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
        <Toaster position="bottom-right" containerAriaLabel={t("toast.region")} />
      </Suspense>
    </TooltipProvider>
  )
}
