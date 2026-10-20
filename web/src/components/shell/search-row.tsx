// The drawer's search row, in the global sidebar and a project's: it opens the ⌘K palette, which
// is how a person jumps to another project, service or pod.
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
      // On a phone the sidebar is in a sheet: close it so the palette is not under it.
      onClick={() => {
        mobileNav.set(false)
        palette.set(true)
      }}
    />
  )
}
