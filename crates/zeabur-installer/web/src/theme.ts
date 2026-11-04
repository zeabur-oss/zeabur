import { themeStore } from "@zeabur-oss/design-system/theme"

export type { ThemeChoice } from "@zeabur-oss/design-system/theme"
export const { followSystemTheme, setTheme, cycleTheme, useTheme } =
  themeStore("zeabur-installer.theme")
