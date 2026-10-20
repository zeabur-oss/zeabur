import { useQuery } from "@tanstack/react-query"
import CheckIcon from "~icons/ph/check"
import DesktopIcon from "~icons/ph/desktop"
import MoonIcon from "~icons/ph/moon"
import ShieldCheckIcon from "~icons/ph/shield-check"
import ShieldSlashIcon from "~icons/ph/shield-slash"
import SignOutIcon from "~icons/ph/sign-out"
import SunIcon from "~icons/ph/sun"

import { sessionQuery, whoamiQuery } from "@/api/queries"
import { dropElevation, retryAsAdmin } from "@/auth/elevation"
import { signOut } from "@/auth/session"
import { localeNames, locales, useI18n, useT } from "@/i18n"
import { setTheme, useTheme, type ThemeChoice } from "@/theme"
import { Avatar } from "@zeabur/design-system/components/avatar"
import {
  Menu,
  MenuContent,
  MenuItem,
  MenuLabel,
  MenuSeparator,
  MenuTrigger,
} from "@zeabur/design-system/components/menu"
import { SidebarPicker } from "@zeabur/design-system/components/sidebar"

const themes: { value: ThemeChoice; icon: typeof SunIcon }[] = [
  { value: "system", icon: DesktopIcon },
  { value: "light", icon: SunIcon },
  { value: "dark", icon: MoonIcon },
]

export function UserMenu() {
  const t = useT()
  const { locale, setLocale } = useI18n()
  const whoami = useQuery(whoamiQuery)
  const session = useQuery(sessionQuery).data
  const theme = useTheme()
  const name = whoami.data?.username ?? ""
  const groups = whoami.data?.groups.join(", ")

  return (
    <Menu>
      <MenuTrigger
        render={
          <SidebarPicker
            avatar={<Avatar size="xs" shape="circle" className="size-6" name={name || undefined} />}
            label={<span translate="no">{name || t("shell.whoamiUnknown")}</span>}
          />
        }
        aria-label={t("shell.account")}
      />
      <MenuContent align="start" className="w-60">
        <div className="flex flex-col px-2.5 py-2">
          <span className="truncate text-sm font-semibold text-main" translate="no">
            {name || t("shell.whoamiUnknown")}
          </span>
          {groups ? (
            <span className="truncate text-xs text-secondary" translate="no" title={groups}>
              {groups}
            </span>
          ) : null}
        </div>
        {session?.elevation === "active" || session?.elevation === "available" ? (
          <>
            <MenuSeparator />
            {session.elevation === "active" ? (
              <MenuItem onClick={() => void dropElevation()}>
                <ShieldSlashIcon aria-hidden="true" />
                {t("elevate.drop")}
              </MenuItem>
            ) : (
              // Granted rights re-read every query (auth/follow-elevation.ts): nothing to retry.
              <MenuItem onClick={() => void retryAsAdmin()}>
                <ShieldCheckIcon aria-hidden="true" />
                {t("elevate.use")}
              </MenuItem>
            )}
          </>
        ) : null}
        <MenuSeparator />
        <MenuLabel>{t("settings.language")}</MenuLabel>
        {locales.map((value) => (
          <MenuItem key={value} onClick={() => setLocale(value)} lang={value}>
            <span className="flex-1">{localeNames[value]}</span>
            {value === locale ? <CheckIcon aria-hidden="true" /> : null}
          </MenuItem>
        ))}
        <MenuSeparator />
        <MenuLabel>{t("shell.theme")}</MenuLabel>
        {themes.map(({ value, icon: ThemeIcon }) => (
          <MenuItem key={value} onClick={() => setTheme(value)}>
            <ThemeIcon aria-hidden="true" />
            <span className="flex-1">{t(`theme.${value}`)}</span>
            {value === theme ? <CheckIcon aria-hidden="true" /> : null}
          </MenuItem>
        ))}
        {session?.signsIn ? (
          <>
            <MenuSeparator />
            <MenuItem onClick={() => void signOut()}>
              <SignOutIcon aria-hidden="true" />
              {t("shell.signOut")}
            </MenuItem>
          </>
        ) : null}
      </MenuContent>
    </Menu>
  )
}
