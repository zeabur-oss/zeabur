// The global sidebar, Zeabur's navigation drawer (the design system's Sidebar): the logo; where
// the dashboard has its workspace picker, the cluster (its name, a link to its page); the search
// row that opens the ⌘K palette; Home and Projects, then Cluster and Settings under Manage; and the
// person's menu pinned at the bottom (admin rights, signing out, language and theme). Inside a
// project the project's sidebar takes its place (app-shell.tsx).
import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import FolderSimpleIcon from "~icons/ph/folder-simple"
import GearSixIcon from "~icons/ph/gear-six"
import HardDrivesIcon from "~icons/ph/hard-drives"
import HouseIcon from "~icons/ph/house"

import { clusterQuery } from "@/api/queries"
import { AdminBadge } from "@/components/shell/admin-badge"
import { SearchRow } from "@/components/shell/search-row"
import { UserMenu } from "@/components/shell/user-menu"
import { useT } from "@/i18n"
import type { Icon } from "@/lib/icon"
import { usePlace } from "@/lib/place"
import { Avatar } from "@zeabur/design-system/components/avatar"
import { Logo } from "@zeabur/design-system/components/brand"
import {
  Sidebar as SidebarRoot,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarHeader,
  SidebarItem,
  SidebarPicker,
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

  // The everyday path first (web/DESIGN.md, "Depth"): where things stand, and the projects.
  const everyday: Item[] = [
    { to: "/", icon: HouseIcon, label: t("shell.home"), active: route === "/" },
    {
      to: "/projects",
      icon: FolderSimpleIcon,
      label: t("shell.projects"),
      active: route === "/projects",
    },
  ]
  // The cluster itself and the preferences.
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
      <ClusterPicker />
      <SearchRow />
      <SidebarContent>
        <SidebarGroup>{everyday.map(navItem)}</SidebarGroup>
        <SidebarSection title={t("shell.manage")}>{manage.map(navItem)}</SidebarSection>
      </SidebarContent>
      <SidebarFooter>
        <AdminBadge />
        <UserMenu />
      </SidebarFooter>
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

/** The workspace picker's place: Zeabur has one cluster for now, so it names the cluster (the
 * kubeconfig's name for it on this machine, kube-apiserver's host in a cluster) and opens its
 * page. */
function ClusterPicker() {
  const t = useT()
  const cluster = useQuery(clusterQuery)
  const name = cluster.data?.name
  return (
    <SidebarPicker
      render={<Link to="/cluster" activeOptions={{ exact: true, includeSearch: false }} />}
      aria-label={name ? `${t("shell.cluster")} ${name}` : t("shell.cluster")}
      avatar={
        <Avatar
          size="xs"
          className="size-6"
          name={name}
          fallbackIcon={<HardDrivesIcon aria-hidden="true" />}
        />
      }
      label={
        <span translate="no" title={name}>
          {name ?? t("shell.cluster")}
        </span>
      }
      trailing={null}
    />
  )
}
