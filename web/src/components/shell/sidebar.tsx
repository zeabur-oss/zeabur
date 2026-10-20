import { Link } from "@tanstack/react-router"
import FolderSimpleIcon from "~icons/ph/folder-simple"
import GearSixIcon from "~icons/ph/gear-six"
import HardDrivesIcon from "~icons/ph/hard-drives"
import HouseIcon from "~icons/ph/house"

import { SearchRow } from "@/components/shell/search-row"
import { ContextMenu } from "@/components/shell/user-menu"
import { useT } from "@/i18n"
import type { Icon } from "@/lib/icon"
import { usePlace } from "@/lib/place"
import { Logo } from "@zeabur/design-system/components/brand"
import {
  Sidebar as SidebarRoot,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarHeader,
  SidebarItem,
  SidebarSection,
} from "@zeabur/design-system/components/sidebar"

type Destination = "/" | "/cluster" | "/projects" | "/settings"

interface Item {
  to: Destination
  icon: Icon
  label: string
  active: boolean
}

export function Sidebar() {
  const t = useT()
  const { route } = usePlace()

  const everyday: Item[] = [
    { to: "/", icon: HouseIcon, label: t("shell.home"), active: route === "/" },
    {
      to: "/projects",
      icon: FolderSimpleIcon,
      label: t("shell.projects"),
      active: route === "/projects",
    },
  ]
  const manage: Item[] = [
    {
      to: "/cluster",
      icon: HardDrivesIcon,
      label: t("shell.cluster"),
      active: route.startsWith("/cluster"),
    },
    {
      to: "/settings",
      icon: GearSixIcon,
      label: t("shell.settings"),
      active: route === "/settings",
    },
  ]

  return (
    <SidebarRoot aria-label={t("shell.nav")}>
      <SidebarHeader>
        <Link
          to="/"
          activeOptions={{ exact: true, includeSearch: false }}
          className="flex h-7 items-center rounded-md outline-none"
          aria-label="Zeabur"
        >
          <Logo className="h-5" aria-hidden="true" />
        </Link>
      </SidebarHeader>
      <SearchRow />
      <SidebarContent>
        <SidebarGroup>{everyday.map(navItem)}</SidebarGroup>
        <SidebarSection title={t("shell.manage")}>{manage.map(navItem)}</SidebarSection>
      </SidebarContent>
      <SidebarFooter>
        <ContextMenu />
      </SidebarFooter>
    </SidebarRoot>
  )
}

function navItem(item: Item) {
  const ItemIcon = item.icon
  return (
    <SidebarItem
      key={item.to}
      // Otherwise the router marks every prefix of the path, or nothing once the page has a search.
      render={<Link to={item.to} activeOptions={{ exact: true, includeSearch: false }} />}
      icon={<ItemIcon aria-hidden="true" />}
      current={item.active}
    >
      {item.label}
    </SidebarItem>
  )
}
