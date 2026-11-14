import { invoke } from "@tauri-apps/api/core"
import { useEffect } from "react"

import { useI18n, type Translate } from "@/i18n"
import { docsUrl } from "@/lib/docs"

const keys = [
  "about",
  "services",
  "hide",
  "hideOthers",
  "showAll",
  "quit",
  "file",
  "newWindow",
  "close",
  "edit",
  "undo",
  "redo",
  "cut",
  "copy",
  "paste",
  "selectAll",
  "view",
  "reload",
  "actualSize",
  "zoomIn",
  "zoomOut",
  "fullScreen",
  "window",
  "minimize",
  "zoom",
  "bringAllToFront",
  "help",
  "docs",
] as const

/** The words of the macOS menu bar; `MenuLabels` in desktop/src/menu.rs. */
function menuLabels(t: Translate): Record<(typeof keys)[number], string> {
  const labels = {} as Record<(typeof keys)[number], string>
  for (const key of keys) labels[key] = t(`desktop.menu.${key}`)
  return labels
}

/** Sets the menu bar in the chooser's language, and again when it changes. */
export function useMenuBar() {
  const { t, locale } = useI18n()
  useEffect(() => {
    const docs = docsUrl(locale, "").replace(/\/\/$/, "/")
    void invoke("set_menu", { labels: menuLabels(t), docs })
  }, [t, locale])
}

/**
 * Keeps the system menu for text the person can edit or has selected; elsewhere a right click does
 * nothing, as on the rest of a native window.
 */
export function keepContextMenuForText() {
  document.addEventListener("contextmenu", (event) => {
    const target = event.target as HTMLElement | null
    if (target?.closest("input, textarea, [contenteditable]")) return
    if (document.getSelection()?.isCollapsed === false) return
    event.preventDefault()
  })
}
