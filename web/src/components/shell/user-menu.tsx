import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { useEffect } from "react"
import CheckIcon from "~icons/ph/check"
import DesktopIcon from "~icons/ph/desktop"
import HardDrivesIcon from "~icons/ph/hard-drives"
import MoonIcon from "~icons/ph/moon"
import PencilSimpleIcon from "~icons/ph/pencil-simple"
import SignOutIcon from "~icons/ph/sign-out"
import SunIcon from "~icons/ph/sun"
import UserCircleIcon from "~icons/ph/user-circle"

import { clusterNameQuery, clusterQuery, sessionQuery, whoamiQuery } from "@/api/queries"
import { signOut } from "@/auth/session"
import { localeNames, locales, useI18n, useT } from "@/i18n"
import { lastKnown } from "@/lib/last-known"
import { nameSourceOf } from "@/lib/cluster-name"
import { personOf } from "@/lib/identity"
import { setTheme, useTheme, type ThemeChoice } from "@/theme"
import { Avatar } from "@zeabur-oss/design-system/components/avatar"
import {
  Menu,
  MenuContent,
  MenuItem,
  MenuLabel,
  MenuSeparator,
  MenuTrigger,
} from "@zeabur-oss/design-system/components/menu"
import { ContextChip } from "@zeabur-oss/design-system/components/context-chip"

const themes: { value: ThemeChoice; icon: typeof SunIcon }[] = [
  { value: "system", icon: DesktopIcon },
  { value: "light", icon: SunIcon },
  { value: "dark", icon: MoonIcon },
]

/** Which cluster this is and who is signed in; the account menu. */
export function ContextMenu() {
  const t = useT()
  const { locale, setLocale } = useI18n()
  const whoami = useQuery(whoamiQuery)
  const session = useQuery(sessionQuery).data
  const cluster = useQuery(clusterQuery).data
  const theme = useTheme()
  const username = whoami.data?.username ?? ""
  const name = personOf(username)
  const groups = whoami.data?.groups.join(", ")
  // While the cluster does not answer, its name as this browser last saw it.
  const place = cluster?.name ?? lastKnown().cluster ?? ""
  const source = cluster ? nameSourceOf(cluster, t) : undefined
  const who = name || t("shell.whoamiUnknown")
  useEffect(() => {
    if (place) document.title = `${place} – Zeabur`
  }, [place])

  return (
    <Menu>
      <MenuTrigger
        render={
          <ContextChip
            place={
              <span translate="no" title={source ? `${place}\n${source}` : place || undefined}>
                {place}
              </span>
            }
            identity={
              <span translate="no" title={username || undefined}>
                {who}
              </span>
            }
            avatar={<Avatar size="xs" shape="circle" className="size-6" name={name || undefined} />}
          />
        }
        aria-label={t("shell.context", place, who)}
      />
      <MenuContent align="start" className="w-60">
        <div className="flex flex-col px-2.5 py-2">
          <span
            className="truncate text-sm font-semibold text-main"
            translate="no"
            title={username || undefined}
          >
            {who}
          </span>
          {groups ? (
            <span className="truncate text-xs text-secondary" translate="no" title={groups}>
              {groups}
            </span>
          ) : null}
        </div>
        <MenuItem render={<Link to="/account" />}>
          <UserCircleIcon aria-hidden="true" />
          {t("shell.account")}
        </MenuItem>
        <MenuItem render={<Link to="/cluster" />}>
          <HardDrivesIcon aria-hidden="true" />
          <span className="flex min-w-0 flex-1 flex-col">
            <span className="truncate" translate="no">
              {place}
            </span>
            {source ? <span className="truncate text-xs text-tertiary">{source}</span> : null}
          </span>
        </MenuItem>
        <RenameCluster />
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

/** A way to rename the cluster, for those who may. */
function RenameCluster() {
  const t = useT()
  const setting = useQuery(clusterNameQuery).data
  if (!setting?.storedIn || setting.canChange !== "allowed") return null
  return (
    <MenuItem render={<Link to="/settings" hash="settings-cluster-name" />}>
      <PencilSimpleIcon aria-hidden="true" />
      {t("shell.renameCluster")}
    </MenuItem>
  )
}
