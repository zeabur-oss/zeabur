// The global sidebar, an icon rail or full width (remembered): Home, Cluster (with its Components
// tab), Projects and Settings under one sliding highlight, adopted projects with their health when
// full, and the person, language and theme at the bottom.
import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import type { ReactElement } from "react"
import DesktopIcon from "~icons/ph/desktop"
import FolderSimpleIcon from "~icons/ph/folder-simple"
import HardDrivesIcon from "~icons/ph/hard-drives"
import HouseIcon from "~icons/ph/house"
import MoonIcon from "~icons/ph/moon"
import ShieldCheckIcon from "~icons/ph/shield-check"
import ShieldSlashIcon from "~icons/ph/shield-slash"
import SidebarSimpleIcon from "~icons/ph/sidebar-simple"
import SignOutIcon from "~icons/ph/sign-out"
import SlidersHorizontalIcon from "~icons/ph/sliders-horizontal"
import SunIcon from "~icons/ph/sun"
import TranslateIcon from "~icons/ph/translate"

import { projectsQuery, sessionQuery, whoamiQuery } from "@/api/queries"
import { dropElevation, retryAsAdmin } from "@/auth/elevation"
import { signOut } from "@/auth/session"
import { ProjectGlyph } from "@/components/shell/project-glyph"
import { StatusDot } from "@/components/status"
import { Menu, MenuContent, MenuItem, MenuTrigger } from "@/components/ui/menu"
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover"
import { Tooltip } from "@/components/ui/tooltip"
import { localeNames, locales, useI18n, useT } from "@/i18n"
import type { Icon } from "@/lib/icon"
import { usePlace } from "@/lib/place"
import { cn } from "@/lib/utils"
import { projectTitle } from "@/model/project"
import { countsHealth } from "@/model/service"
import { cycleTheme, useTheme } from "@/theme"

/** The sill: a long floor edge with a shorter eave above it, set to the right. */
function Logo({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true" className={cn("text-brand", className)}>
      <path
        d="M2.5 17.5h19M9.5 10.5h12"
        stroke="currentColor"
        strokeWidth="2.6"
        strokeLinecap="round"
        fill="none"
      />
    </svg>
  )
}

function Wordmark({ rail }: { rail?: boolean }) {
  return (
    <span className="flex items-center gap-2">
      <Logo className="size-5.5 shrink-0" />
      {rail ? null : (
        <span className="type-heading font-semibold tracking-tight text-brand">engawa</span>
      )}
    </span>
  )
}

type Destination = "/" | "/cluster" | "/projects" | "/settings"

interface Item {
  to: Destination
  icon: Icon
  label: string
  active: boolean
}

export function Sidebar({
  rail,
  onToggle,
  mobile,
}: {
  rail: boolean
  onToggle?: () => void
  mobile?: boolean
}) {
  const t = useT()
  const place = usePlace()
  const { route } = place

  const items: Item[] = [
    { to: "/", icon: HouseIcon, label: t("shell.home"), active: route === "/" },
    {
      to: "/cluster",
      icon: HardDrivesIcon,
      label: t("shell.cluster"),
      active: route.startsWith("/cluster"),
    },
    {
      to: "/projects",
      icon: FolderSimpleIcon,
      label: t("shell.projects"),
      active: route.startsWith("/projects"),
    },
    {
      to: "/settings",
      icon: SlidersHorizontalIcon,
      label: t("shell.settings"),
      active: route === "/settings",
    },
  ]
  const current = items.findIndex((item) => item.active)
  // One step is an item plus the gap below it: 40 + 4 px on the rail, 32 + 2 px when full.
  const step = rail ? 2.75 : 2.125

  return (
    <nav
      aria-label={t("shell.nav")}
      className={cn(
        "flex h-full shrink-0 flex-col bg-sidebar text-sm",
        mobile ? "w-full" : "border-r border-sidebar-border transition-width duration-200",
        !mobile && (rail ? "w-14" : "w-60"),
      )}
    >
      <div
        className={cn(
          "flex h-14 shrink-0 items-center px-4",
          rail && "justify-center px-0",
          mobile && "pr-12",
        )}
      >
        <Link
          to="/"
          className="flex h-10 items-center rounded-lg px-2 outline-none focus-visible:ring-3 focus-visible:ring-ring"
          aria-label="engawa"
        >
          <Wordmark rail={rail} />
        </Link>
      </div>

      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto py-2">
        <div className="relative">
          <div
            aria-hidden="true"
            className={cn(
              "slide pointer-events-none absolute top-0 translate-y-(--slide-y) rounded-lg bg-muted",
              rail ? "left-2 size-10 ring-1 ring-border ring-inset" : "inset-x-2 h-8",
              current < 0 && "opacity-0",
            )}
            style={{ "--slide-y": `${Math.max(current, 0) * step}rem` }}
          />
          <ul
            className={cn("relative flex flex-col", rail ? "items-center gap-1" : "gap-0.5 px-2")}
          >
            {items.map((item) => (
              <li key={item.to}>
                <NavItem item={item} rail={rail} />
              </li>
            ))}
          </ul>
        </div>
        {rail ? null : <ProjectShortcuts current={place.project} />}
      </div>

      <Footer rail={rail} onToggle={onToggle} mobile={mobile} />
    </nav>
  )
}

function RailTip({ label, children }: { label: string; children: ReactElement }) {
  return (
    <Tooltip content={label} side="right">
      {children}
    </Tooltip>
  )
}

function NavItem({ item, rail }: { item: Item; rail: boolean }) {
  const ItemIcon = item.icon
  const link = (
    <Link
      to={item.to}
      aria-current={item.active ? "page" : undefined}
      aria-label={rail ? item.label : undefined}
      className={cn(
        "flex items-center gap-2.5 rounded-lg text-muted-foreground outline-none transition-colors hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring",
        rail ? "size-10 justify-center" : "h-8 px-2",
        item.active && "font-medium text-foreground",
      )}
    >
      <ItemIcon aria-hidden="true" className="size-4 shrink-0" />
      {rail ? null : <span className="truncate">{item.label}</span>}
    </Link>
  )
  return rail ? <RailTip label={item.label}>{link}</RailTip> : link
}

/** Adopted projects, each with its health: a quiet list under the main items. The counts come with
 * the project list, so the sidebar reads no project document. */
function ProjectShortcuts({ current }: { current?: string }) {
  const t = useT()
  const projects = useQuery(projectsQuery)
  const adopted = (projects.data?.items ?? []).filter((p) => p.adopted)
  if (adopted.length === 0) return null
  return (
    <div className="flex flex-col gap-0.5 px-2">
      <div className="px-2 pb-1 type-meta font-medium text-muted-foreground">
        {t("shell.projects")}
      </div>
      <ul className="flex flex-col gap-0.5">
        {adopted.map((p) => {
          const summary = p.health ?? undefined
          return (
            <li key={p.name}>
              <Link
                to="/projects/$project"
                params={{ project: p.name }}
                aria-current={current === p.name ? "page" : undefined}
                className={cn(
                  "flex h-8 items-center gap-2 rounded-lg px-2 text-muted-foreground outline-none hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring",
                  current === p.name && "font-medium text-foreground",
                )}
              >
                <ProjectGlyph icon={p.icon} className="text-current" />
                <span className="truncate" translate="no">
                  {projectTitle(p, p.name)}
                </span>
                {summary && summary.services > 0 ? (
                  <span className="ml-auto flex items-center gap-1.5 type-meta">
                    {summary.healthy}/{summary.services}
                    <StatusDot health={countsHealth(summary)} />
                  </span>
                ) : null}
              </Link>
            </li>
          )
        })}
      </ul>
    </div>
  )
}

function Footer({
  rail,
  onToggle,
  mobile,
}: {
  rail: boolean
  onToggle?: () => void
  mobile?: boolean
}) {
  const t = useT()
  const { locale, setLocale } = useI18n()
  const whoami = useQuery(whoamiQuery)
  const session = useQuery(sessionQuery).data
  const theme = useTheme()
  const ThemeIcon = theme === "dark" ? MoonIcon : theme === "light" ? SunIcon : DesktopIcon
  const name = whoami.data?.username ?? ""
  const groups = whoami.data?.groups.join(", ")
  const initial = name.slice(0, 1).toLocaleUpperCase()
  const iconButton =
    "flex size-10 items-center justify-center rounded-lg text-muted-foreground outline-none hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring"

  const language = (
    <Popover>
      <PopoverTrigger
        variant="plain"
        className="flex size-10 items-center justify-center"
        aria-label={t("settings.language")}
      >
        <TranslateIcon className="size-4" aria-hidden="true" />
      </PopoverTrigger>
      <PopoverContent side="top" className="w-48">
        <ul className="flex flex-col">
          {locales.map((value) => (
            <li key={value}>
              <button
                type="button"
                onClick={() => setLocale(value)}
                lang={value}
                aria-pressed={value === locale}
                className="flex h-8 w-full items-center gap-2 rounded-lg px-2 text-left outline-none hover:bg-muted focus-visible:bg-muted aria-pressed:font-medium"
              >
                {localeNames[value]}
              </button>
            </li>
          ))}
        </ul>
      </PopoverContent>
    </Popover>
  )
  const themeButton = (
    <Tooltip content={t(`theme.${theme}`)} side={rail ? "right" : "top"}>
      <button
        type="button"
        className={iconButton}
        onClick={cycleTheme}
        aria-label={t(`theme.${theme}`)}
      >
        <ThemeIcon className="size-4" aria-hidden="true" />
      </button>
    </Tooltip>
  )
  const toggle = onToggle ? (
    <Tooltip content={rail ? t("shell.expandSidebar") : t("shell.collapseSidebar")} side="right">
      <button
        type="button"
        className={iconButton}
        onClick={onToggle}
        aria-label={rail ? t("shell.expandSidebar") : t("shell.collapseSidebar")}
        aria-pressed={!rail}
      >
        <SidebarSimpleIcon className="size-4" aria-hidden="true" />
      </button>
    </Tooltip>
  ) : null
  const avatar = (
    <span className="flex size-8 shrink-0 items-center justify-center rounded-full bg-muted text-xs font-semibold">
      {initial}
    </span>
  )

  const person = (
    <>
      {avatar}
      <span className="flex min-w-0 flex-col">
        <span className="truncate font-medium" translate="no">
          {name || t("shell.whoamiUnknown")}
        </span>
        <span className="truncate type-meta text-muted-foreground" translate="no">
          {groups}
        </span>
      </span>
    </>
  )
  // Where people sign in (the in-cluster server), the person opens a menu to sign out.
  const accountItems = (
    <MenuContent align="start">
      {session?.elevation === "active" ? (
        <MenuItem onClick={() => void dropElevation()}>
          <ShieldSlashIcon aria-hidden="true" />
          {t("elevate.drop")}
        </MenuItem>
      ) : null}
      {/* The way in that every page has, while admin rights are offered. Once they are granted,
          every query is read again (auth/follow-elevation.ts), so nothing needs retrying. */}
      {session?.elevation === "available" ? (
        <MenuItem onClick={() => void retryAsAdmin()}>
          <ShieldCheckIcon aria-hidden="true" />
          {t("elevate.use")}
        </MenuItem>
      ) : null}
      <MenuItem onClick={() => void signOut()}>
        <SignOutIcon aria-hidden="true" />
        {t("shell.signOut")}
      </MenuItem>
    </MenuContent>
  )
  const accountButton =
    "flex items-center gap-2 rounded-lg text-left outline-none hover:bg-muted focus-visible:ring-3 focus-visible:ring-ring aria-expanded:bg-muted"

  return (
    <div className={cn("flex flex-col gap-1 p-2", rail ? "items-center" : "border-t")}>
      {rail ? (
        <>
          {language}
          {themeButton}
          {toggle}
          {session?.signsIn ? (
            <Menu>
              <Tooltip content={`${name} (${groups ?? ""})`} side="right">
                <MenuTrigger
                  render={
                    <button type="button" className={cn(accountButton, "mt-1 rounded-full")} />
                  }
                  aria-label={t("shell.account")}
                >
                  {avatar}
                </MenuTrigger>
              </Tooltip>
              {accountItems}
            </Menu>
          ) : (
            <Tooltip content={`${name} (${groups ?? ""})`} side="right">
              <span className="mt-1">{avatar}</span>
            </Tooltip>
          )}
        </>
      ) : (
        <>
          {session?.signsIn ? (
            <Menu>
              <MenuTrigger
                render={<button type="button" className={cn(accountButton, "px-1.5 py-1.5")} />}
                aria-label={t("shell.account")}
              >
                {person}
              </MenuTrigger>
              {accountItems}
            </Menu>
          ) : (
            <div className="flex items-center gap-2 rounded-lg px-1.5 py-1.5">{person}</div>
          )}
          <div className={cn("flex items-center gap-0.5", mobile && "justify-start")}>
            {language}
            {themeButton}
            <span className="flex-1" />
            {toggle}
          </div>
        </>
      )}
    </div>
  )
}
