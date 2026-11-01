import { Link, Outlet, useLocation } from "@tanstack/react-router"
import { lazy, Suspense } from "react"
import ListIcon from "~icons/ph/list"
import MagnifyingGlassIcon from "~icons/ph/magnifying-glass"
import XIcon from "~icons/ph/x"

import { RouteProgress } from "@/components/shell/route-progress"
import { PlanFallback } from "@/components/plan/fallback"
import { CommandPalette } from "@/components/shell/command-palette"
import { DockSlot } from "@/components/shell/dock-slot"
import { Sidebar } from "@/components/shell/sidebar"
import { useT } from "@/i18n"
import { adding } from "@/lib/adding"
import { usePlace } from "@/lib/place"
import { mobileNav, palette } from "@/lib/stores"
import { useTheme } from "@/theme"
import { Logo } from "@zeabur-oss/design-system/components/brand"
import { Button } from "@zeabur-oss/design-system/components/button"
import { Dialog, DialogContent, DialogTitle } from "@zeabur-oss/design-system/components/dialog"
import { AppFrame, FramePanel, FrameSidebar } from "@zeabur-oss/design-system/components/frame"
import { usePageTransition } from "@zeabur-oss/design-system/components/page-transition"
import { TooltipProvider } from "@zeabur-oss/design-system/components/tooltip"

// Lazy: Sonner stays out of the first page load (scripts/ui-budget.sh).
const Toaster = lazy(() =>
  import("@zeabur-oss/design-system/components/sonner").then((m) => ({ default: m.Toaster })),
)
// Only project pages show the project sidebar, with its "New" menu and the services' states.
const ProjectSidebar = lazy(() =>
  import("@/components/shell/project-sidebar").then((m) => ({ default: m.ProjectSidebar })),
)
// The "New" forms load when one opens.
const AddHost = lazy(() => import("@/components/add/host").then((m) => ({ default: m.AddHost })))

export function AppShell() {
  const t = useT()
  const place = usePlace()
  const mobileOpen = mobileNav.useValue()
  const add = adding.useValue()
  const theme = useTheme()
  const pathname = useLocation({ select: (location) => location.pathname })
  const content = usePageTransition<HTMLElement>(pathname)
  const nav = place.project ? (
    <Suspense fallback={null}>
      <ProjectSidebar project={place.project} />
    </Suspense>
  ) : (
    <Sidebar />
  )

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
          <RouteProgress />
          <PhoneBar />
          <main
            ref={content}
            id="content"
            tabIndex={-1}
            className="relative flex min-h-0 flex-1 flex-col overflow-x-hidden overflow-y-auto overscroll-contain outline-none page-transition"
          >
            <Outlet />
          </main>
          <DockSlot />
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
      {add ? (
        <Suspense fallback={null}>
          <AddHost key={`${add.project}/${add.what}`} project={add.project} what={add.what} />
        </Suspense>
      ) : null}
      <Suspense fallback={<div aria-live="polite" />}>
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
