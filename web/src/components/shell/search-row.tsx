import { preloadPalette } from "@/components/shell/command-palette"
import { useT } from "@/i18n"
import { mobileNav, palette } from "@/lib/stores"
import { SidebarSearch } from "@zeabur/design-system/components/sidebar"

const mac = typeof navigator !== "undefined" && /Mac|iPhone|iPad|iPod/.test(navigator.platform)

export function SearchRow() {
  const t = useT()
  return (
    <SidebarSearch
      label={t("shell.search")}
      shortcut={mac ? "⌘K" : "Ctrl K"}
      aria-label={t("shell.search")}
      title={t("shell.jumpTo")}
      onPointerEnter={preloadPalette}
      onFocus={preloadPalette}
      // On a phone the sidebar is a sheet that would cover the palette.
      onClick={() => {
        mobileNav.set(false)
        palette.set(true)
      }}
    />
  )
}
