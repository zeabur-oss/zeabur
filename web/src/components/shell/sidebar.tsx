// The global sidebar on the window ground (agent-playground's anatomy, the design system's
// Sidebar): the logo with search (the ⌘K palette) beside it, then the everyday path, Home and
// Projects; the adopted projects with their health in a section that folds; Cluster and Settings
// in a Manage section, folded until opened; and the person, language and theme at the bottom. It
// folds away from the panel's header (app-shell.tsx).
import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import DesktopIcon from "~icons/ph/desktop"
import FolderSimpleIcon from "~icons/ph/folder-simple"
import HardDrivesIcon from "~icons/ph/hard-drives"
import HouseIcon from "~icons/ph/house"
import MagnifyingGlassIcon from "~icons/ph/magnifying-glass"
import MoonIcon from "~icons/ph/moon"
import ShieldCheckIcon from "~icons/ph/shield-check"
import ShieldSlashIcon from "~icons/ph/shield-slash"
import SignOutIcon from "~icons/ph/sign-out"
import SlidersHorizontalIcon from "~icons/ph/sliders-horizontal"
import SunIcon from "~icons/ph/sun"
import TranslateIcon from "~icons/ph/translate"

import { projectsQuery, sessionQuery, whoamiQuery } from "@/api/queries"
import { dropElevation, retryAsAdmin } from "@/auth/elevation"
import { signOut } from "@/auth/session"
import { ProjectGlyph } from "@/components/shell/project-glyph"
import { StatusDot } from "@/components/status"
import { localeNames, locales, useI18n, useT } from "@/i18n"
import type { Icon } from "@/lib/icon"
import { usePlace } from "@/lib/place"
import { mobileNav, palette, remembered } from "@/lib/stores"
import { cn } from "@/lib/utils"
import { projectTitle } from "@/model/project"
import { countsHealth } from "@/model/service"
import { cycleTheme, useTheme } from "@/theme"
import { Logo } from "@zeabur/design-system/components/brand"
import { Button } from "@zeabur/design-system/components/button"
import { Menu, MenuContent, MenuItem, MenuTrigger } from "@zeabur/design-system/components/menu"
import { Popover, PopoverContent, PopoverTrigger } from "@zeabur/design-system/components/popover"
import {
  Sidebar as SidebarRoot,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarHeader,
  SidebarItem,
  SidebarSection,
  sidebarItemVariants,
} from "@zeabur/design-system/components/sidebar"
import { Tooltip } from "@zeabur/design-system/components/tooltip"

type Destination = "/" | "/cluster" | "/projects" | "/settings"

const manageSection = remembered("zeabur.sidebar.manage", false)

interface Item {
  to: Destination
  icon: Icon
  label: string
  active: boolean
}

export function Sidebar() {
  const t = useT()
  const place = usePlace()
  const { route } = place

  // The everyday path first (web/DESIGN.md, "Depth"): where things stand, and the projects.
  const everyday: Item[] = [
    { to: "/", icon: HouseIcon, label: t("shell.home"), active: route === "/" },
    {
      to: "/projects",
      icon: FolderSimpleIcon,
      label: t("shell.projects"),
      // Inside a project the project's column holds the current row; Projects is the list.
      active: route.startsWith("/projects") && !place.project,
    },
  ]
  // The cluster itself and the preferences: folded at the bottom until opened.
  const manage: Item[] = [
    {
      to: "/cluster",
      icon: HardDrivesIcon,
      label: t("shell.cluster"),
      active: route.startsWith("/cluster"),
    },
    {
      to: "/settings",
      icon: SlidersHorizontalIcon,
      label: t("shell.settings"),
      active: route === "/settings",
    },
  ]
  const manageOpen = manageSection.useValue()
  const manageHolds = manage.some((item) => item.active)

  return (
    <SidebarRoot aria-label={t("shell.nav")}>
      <SidebarHeader>
        <Link
          to="/"
          activeOptions={{ exact: true, includeSearch: false }}
          className="flex h-7 items-center rounded-md outline-none"
          aria-label="zeabur"
        >
          <Logo className="h-4.5" aria-hidden="true" />
        </Link>
        <Tooltip content={`${t("shell.jumpTo")} ⌘K`}>
          <Button
            variant="muted"
            size="icon-sm"
            className="ml-auto"
            aria-label={t("shell.search")}
            // On a phone the sidebar is in a sheet: close it so the palette is not under it.
            onClick={() => {
              mobileNav.set(false)
              palette.set(true)
            }}
          >
            <MagnifyingGlassIcon aria-hidden="true" className="size-4" />
          </Button>
        </Tooltip>
      </SidebarHeader>
      <SidebarContent>
        <SidebarGroup>{everyday.map(navItem)}</SidebarGroup>
        <ProjectShortcuts current={place.project} />
        <SidebarSection
          title={t("shell.manage")}
          // Open while it holds the page shown: the current row is never folded away.
          open={manageOpen || manageHolds}
          // Held open by the page shown, a click is not a choice to remember.
          onOpenChange={(open) => {
            if (!manageHolds) manageSection.set(open)
          }}
        >
          {manage.map(navItem)}
        </SidebarSection>
      </SidebarContent>
      <Footer />
    </SidebarRoot>
  )
}

function navItem(item: Item) {
  const ItemIcon = item.icon
  return (
    <SidebarItem
      key={item.to}
      // The exact path, whatever the search: the router would otherwise mark every link whose path
      // is a prefix of the page's, or none once the page has a search.
      render={<Link to={item.to} activeOptions={{ exact: true, includeSearch: false }} />}
      icon={<ItemIcon aria-hidden="true" />}
      current={item.active}
    >
      {item.label}
    </SidebarItem>
  )
}

/** Adopted projects, each with its health, in a section that folds; the one the page is in is
 * an ancestor, not current. The counts come with the project list, so the sidebar reads no project
 * document. */
function ProjectShortcuts({ current }: { current?: string }) {
  const t = useT()
  const projects = useQuery(projectsQuery)
  const adopted = (projects.data?.items ?? []).filter((p) => p.adopted)
  if (adopted.length === 0) return null
  return (
    <SidebarSection title={t("shell.projects")}>
      {adopted.map((p) => {
        const summary = p.health ?? undefined
        return (
          <SidebarItem
            key={p.name}
            render={
              <Link
                to="/projects/$project"
                params={{ project: p.name }}
                activeOptions={{ exact: true, includeSearch: false }}
              />
            }
            icon={<ProjectGlyph icon={p.icon} className="text-current" />}
            // The page is somewhere in this project: its column holds the one current row.
            ancestor={current === p.name}
            trailing={
              summary && summary.services > 0 ? (
                <>
                  {summary.healthy}/{summary.services}
                  <StatusDot health={countsHealth(summary)} />
                </>
              ) : undefined
            }
          >
            <span translate="no">{projectTitle(p, p.name)}</span>
          </SidebarItem>
        )
      })}
    </SidebarSection>
  )
}

function Footer() {
  const t = useT()
  const { locale, setLocale } = useI18n()
  const whoami = useQuery(whoamiQuery)
  const session = useQuery(sessionQuery).data
  const theme = useTheme()
  const ThemeIcon = theme === "dark" ? MoonIcon : theme === "light" ? SunIcon : DesktopIcon
  const name = whoami.data?.username ?? ""
  const groups = whoami.data?.groups.join(", ")
  const initial = name.slice(0, 1).toLocaleUpperCase()

  const person = (
    <>
      <span className="flex size-6 shrink-0 items-center justify-center rounded-full bg-selected type-micro font-semibold text-foreground">
        {initial}
      </span>
      <span className="flex min-w-0 flex-1 flex-col">
        <span className="truncate type-meta text-foreground" translate="no">
          {name || t("shell.whoamiUnknown")}
        </span>
        <span className="truncate type-micro text-muted-foreground" translate="no">
          {groups}
        </span>
      </span>
    </>
  )

  return (
    <SidebarFooter>
      {/* Where people sign in (the in-cluster server), the person opens a menu to sign out. */}
      {session?.signsIn ? (
        <Menu>
          <MenuTrigger
            render={
              <button type="button" className={cn(sidebarItemVariants(), "h-auto gap-2 py-1.5")} />
            }
            aria-label={t("shell.account")}
          >
            {person}
          </MenuTrigger>
          <MenuContent align="start">
            {session.elevation === "active" ? (
              <MenuItem onClick={() => void dropElevation()}>
                <ShieldSlashIcon aria-hidden="true" />
                {t("elevate.drop")}
              </MenuItem>
            ) : null}
            {/* The way in that every page has, while admin rights are offered. Once they are
                granted, every query is read again (auth/follow-elevation.ts), so nothing needs
                retrying. */}
            {session.elevation === "available" ? (
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
        </Menu>
      ) : (
        <div className="flex items-center gap-2 px-2 py-1.5">{person}</div>
      )}
      <div className="flex items-center gap-0.5">
        <Popover>
          <PopoverTrigger
            variant="ghost"
            className="flex size-7 items-center justify-center"
            aria-label={t("settings.language")}
          >
            <TranslateIcon className="size-3.5" aria-hidden="true" />
          </PopoverTrigger>
          <PopoverContent side="top" align="start" className="w-48">
            <ul className="flex flex-col">
              {locales.map((value) => (
                <li key={value}>
                  <button
                    type="button"
                    onClick={() => setLocale(value)}
                    lang={value}
                    aria-pressed={value === locale}
                    className="flex h-8 w-full items-center gap-2 rounded-md px-2 text-left outline-none hover:bg-accent focus-visible:bg-accent aria-pressed:text-foreground aria-pressed:font-semibold"
                  >
                    {localeNames[value]}
                  </button>
                </li>
              ))}
            </ul>
          </PopoverContent>
        </Popover>
        <Tooltip content={t(`theme.${theme}`)}>
          <Button
            variant="muted"
            size="icon-sm"
            onClick={cycleTheme}
            aria-label={t(`theme.${theme}`)}
          >
            <ThemeIcon aria-hidden="true" />
          </Button>
        </Tooltip>
      </div>
    </SidebarFooter>
  )
}
