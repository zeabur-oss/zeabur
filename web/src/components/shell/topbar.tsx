// The panel's top bar (the design system's PanelHeader): the sidebar's toggle (the phone menu below
// `md`) and a breadcrumb (Cluster / shop ▾ / worker ▾: each crumb a link, each ▾ a separate
// switcher). Search is beside the sidebar's logo.
import { Link } from "@tanstack/react-router"
import type { ReactNode } from "react"

import { AdminBadge } from "@/components/shell/admin-badge"
import { ProjectSwitcher, ServiceSwitcher } from "@/components/shell/switchers"
import { useT } from "@/i18n"
import { serviceLink, usePlace } from "@/lib/place"
import { useProjectSummary } from "@/lib/project-name"
import { mobileNav, setSidebarCollapsed, sidebarCollapsed } from "@/lib/stores"
import { cn } from "@/lib/utils"
import { projectTitle } from "@/model/project"
import { Button } from "@engawahq/design-system/components/button"
import { PanelHeader } from "@engawahq/design-system/components/frame"
import { SidebarToggleIcon } from "@engawahq/design-system/components/sidebar"
import { Tooltip } from "@engawahq/design-system/components/tooltip"

const crumbLink = "min-w-0 truncate rounded-md px-1 outline-none hover:text-foreground"

export function TopBar() {
  const t = useT()
  const place = usePlace()
  const collapsed = sidebarCollapsed.useValue()
  const toggleLabel = collapsed ? t("shell.expandSidebar") : t("shell.collapseSidebar")
  // The service crumb belongs to the service page; the Architecture quick look keeps the project's.
  const serviceKey = place.entry ? undefined : place.service
  const [kind, name] = serviceKey?.split("/") ?? []
  const project = useProjectSummary(place.project ?? "")

  let crumbs: ReactNode
  if (place.project) {
    crumbs = (
      <>
        <Link
          to="/cluster"
          activeOptions={{ exact: true, includeSearch: false }}
          className={cn(crumbLink, "hidden sm:inline")}
        >
          {t("shell.cluster")}
        </Link>
        <Slash className="hidden sm:inline" />
        <Link
          to="/projects/$project"
          params={{ project: place.project }}
          activeOptions={{ exact: true, includeSearch: false }}
          className={cn(crumbLink, !serviceKey && "font-medium text-foreground")}
          translate="no"
        >
          {projectTitle(project, place.project)}
        </Link>
        <ProjectSwitcher current={place.project} />
        {kind && name ? (
          <>
            <Slash />
            <Link
              {...serviceLink(place.project, { kind, name }, { tab: place.tab })}
              className={cn(crumbLink, "font-medium text-foreground")}
              translate="no"
            >
              {name}
            </Link>
            <ServiceSwitcher project={place.project} current={serviceKey ?? ""} />
          </>
        ) : null}
      </>
    )
  } else if (place.route === "/settings") {
    crumbs = <span className="px-1 font-medium text-foreground">{t("shell.settings")}</span>
  } else if (place.route === "/projects") {
    crumbs = <span className="px-1 font-medium text-foreground">{t("shell.projects")}</span>
  } else if (place.route === "/cluster/components") {
    crumbs = (
      <>
        <Link
          to="/cluster"
          activeOptions={{ exact: true, includeSearch: false }}
          className={crumbLink}
        >
          {t("shell.cluster")}
        </Link>
        <Slash />
        <span className="px-1 font-medium text-foreground">{t("settings.components")}</span>
      </>
    )
  } else if (place.route === "/cluster") {
    crumbs = <span className="px-1 font-medium text-foreground">{t("shell.cluster")}</span>
  } else {
    crumbs = <span className="px-1 font-medium text-foreground">{t("shell.home")}</span>
  }

  return (
    <PanelHeader>
      <Button
        variant="muted"
        size="icon-sm"
        className="md:hidden"
        onClick={() => mobileNav.set(true)}
        aria-label={t("shell.menu")}
      >
        <SidebarToggleIcon collapsed />
      </Button>
      <Tooltip content={`${toggleLabel} ⌘\\`}>
        <Button
          variant="muted"
          size="icon-sm"
          className="hidden md:inline-flex"
          onClick={() => setSidebarCollapsed(!collapsed)}
          aria-label={toggleLabel}
          aria-pressed={!collapsed}
        >
          <SidebarToggleIcon collapsed={collapsed} />
        </Button>
      </Tooltip>
      <nav
        aria-label={t("shell.breadcrumb")}
        className="flex min-w-0 flex-1 items-center gap-1 text-muted-foreground"
      >
        {crumbs}
      </nav>
      <AdminBadge />
    </PanelHeader>
  )
}

function Slash({ className }: { className?: string }) {
  return (
    <span aria-hidden="true" className={cn("px-0.5 text-muted-foreground/60", className)}>
      /
    </span>
  )
}
