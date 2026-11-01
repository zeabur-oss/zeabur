// The contents load when the browser is idle or the search row is pointed at, keeping them off the
// first page load.
import { lazy, Suspense, useEffect } from "react"

import { Dialog, DialogContent, DialogTitle } from "@zeabur-oss/design-system/components/dialog"
import { useT } from "@/i18n"
import { palette } from "@/lib/stores"

const loadBody = () => import("@/components/shell/palette-body")
const PaletteBody = lazy(() => loadBody().then((m) => ({ default: m.PaletteBody })))

export function preloadPalette() {
  void loadBody()
}

export function CommandPalette() {
  const open = palette.useValue()
  const t = useT()

  useEffect(() => {
    const onKey = (event: globalThis.KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault()
        palette.set(!palette.get())
      }
    }
    window.addEventListener("keydown", onKey)
    if (!("requestIdleCallback" in window)) {
      return () => window.removeEventListener("keydown", onKey)
    }
    const idle = requestIdleCallback(preloadPalette, { timeout: 4000 })
    return () => {
      window.removeEventListener("keydown", onKey)
      cancelIdleCallback(idle)
    }
  }, [])

  return (
    <Dialog open={open} onOpenChange={(next) => palette.set(next)}>
      <DialogContent aria-describedby={undefined} position="top">
        <DialogTitle className="sr-only">{t("palette.title")}</DialogTitle>
        <Suspense fallback={<PaletteFallback />}>
          <PaletteBody />
        </Suspense>
      </DialogContent>
    </Dialog>
  )
}

function PaletteFallback() {
  return (
    <div aria-hidden="true">
      <div className="box-content h-13 border-b" />
      <div className="h-palette" />
      <div className="border-t bg-muted/40 py-2 type-micro leading-normal">&nbsp;</div>
    </div>
  )
}
