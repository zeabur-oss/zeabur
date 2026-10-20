import { Link, Outlet } from "@tanstack/react-router"
import { lazy, Suspense } from "react"
import ListIcon from "~icons/ph/list"
import MagnifyingGlassIcon from "~icons/ph/magnifying-glass"
import XIcon from "~icons/ph/x"

import { elevation } from "@/auth/elevation"
import { PlanFallback } from "@/components/plan/fallback"
import { CommandPalette } from "@/components/shell/command-palette"
import { ProjectSidebar } from "@/components/shell/project-sidebar"
import { Sidebar } from "@/components/shell/sidebar"
import { useT } from "@/i18n"
import { usePlace } from "@/lib/place"
import { mobileNav, palette } from "@/lib/stores"
import { useTheme } from "@/theme"
import { Logo } from "@zeabur/design-system/components/brand"
import { Button } from "@zeabur/design-system/components/button"
import { Dialog, DialogContent, DialogTitle } from "@zeabur/design-system/components/dialog"
import { AppFrame, FramePanel, FrameSidebar } from "@zeabur/design-system/components/frame"
import { TooltipProvider } from "@zeabur/design-system/components/tooltip"

// Lazy: Sonner stays out of the first page load (scripts/ui-budget.sh).
const Toaster = lazy(() =>
  import("@zeabur/design-system/components/sonner").then((m) => ({ default: m.Toaster })),
)
// "Retry as admin" starts from a failure toast, so its dialog loads only then.
const ElevationDialog = lazy(() =>
  import("@/components/elevation-dialog").then((m) => ({ default: m.ElevationDialog })),
)

export function AppShell() {
  const t = useT()
  const place = usePlace()
  const mobileOpen = mobileNav.useValue()
  const elevating = elevation.useValue()
  const theme = useTheme()
  const nav = place.project ? <ProjectSidebar project={place.project} /> : <Sidebar />

  return (
    <TooltipProvider>
      <AppFrame>
        <a
          href="#content"
          className="sr-only z-50 rounded-md bg-background px-3 py-2 text-sm focus:not-sr-only focus:fixed focus:top-2 focus:left-2"
        >
          {t("shell.skip")}
        </a>
        <FrameSidebar>{nav}</FrameSidebar>
        <FramePanel>
          <PhoneBar />
          <main
            id="content"
            tabIndex={-1}
            className="relative flex min-h-0 flex-1 flex-col overflow-y-auto overscroll-contain outline-none"
          >
            <Outlet />
          </main>
        </FramePanel>
      </AppFrame>
      <Dialog open={mobileOpen} onOpenChange={(open) => mobileNav.set(open)}>
        <DialogContent variant="sheet" className="flex flex-col" aria-describedby={undefined}>
          <DialogTitle className="sr-only">{t("shell.menu")}</DialogTitle>
          <Button
            variant="muted"
            size="icon-sm"
            onClick={() => mobileNav.set(false)}
            className="absolute top-5 right-3 z-10"
            aria-label={t("common.close")}
          >
            <XIcon aria-hidden="true" />
          </Button>
          <div
            onClickCapture={(event) => {
              if ((event.target as HTMLElement).closest("a")) mobileNav.set(false)
            }}
            className="flex min-h-0 flex-1 flex-col"
          >
            {nav}
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

function PhoneBar() {
  const t = useT()
  return (
    <header className="flex h-14 shrink-0 items-center gap-2 border-b border-main bg-card px-3 md:hidden">
      <Button
        variant="muted"
        size="icon-sm"
        onClick={() => mobileNav.set(true)}
        aria-label={t("shell.menu")}
      >
        <ListIcon aria-hidden="true" />
      </Button>
      <Link to="/" className="flex h-7 items-center rounded-md outline-none" aria-label="Zeabur">
        <Logo className="h-5" aria-hidden="true" />
      </Link>
      <Button
        variant="muted"
        size="icon-sm"
        className="ml-auto"
        onClick={() => palette.set(true)}
        aria-label={t("shell.search")}
      >
        <MagnifyingGlassIcon aria-hidden="true" />
      </Button>
    </header>
  )
}
