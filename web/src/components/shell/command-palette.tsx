// The ⌘K palette: jump to a project, service, pod (its logs) or domain, run a few actions, and see
// recent items. Its contents load when it first opens, so the first page load carries only this.
import { lazy, Suspense, useEffect } from "react"

import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog"
import { useT } from "@/i18n"
import { palette } from "@/lib/stores"

const PaletteBody = lazy(() =>
  import("@/components/shell/palette-body").then((m) => ({ default: m.PaletteBody })),
)

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
    return () => window.removeEventListener("keydown", onKey)
  }, [])

  return (
    <Dialog open={open} onOpenChange={(next) => palette.set(next)}>
      <DialogContent aria-describedby={undefined}>
        <DialogTitle className="sr-only">{t("palette.title")}</DialogTitle>
        {open ? (
          <Suspense fallback={<div className="h-13" />}>
            <PaletteBody />
          </Suspense>
        ) : null}
      </DialogContent>
    </Dialog>
  )
}
