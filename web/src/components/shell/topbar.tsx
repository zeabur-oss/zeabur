// The top bar: the phone menu button, a breadcrumb (Cluster / shop ▾ / worker ▾: each crumb a link,
// each ▾ a separate switcher) and "Jump to…".
import { Link } from "@tanstack/react-router"
import type { ReactNode } from "react"
import ListIcon from "~icons/ph/list"
import MagnifyingGlassIcon from "~icons/ph/magnifying-glass"

import { AdminBadge } from "@/components/shell/admin-badge"
import { ProjectSwitcher, ServiceSwitcher } from "@/components/shell/switchers"
import { useT } from "@/i18n"
import { serviceLink, usePlace } from "@/lib/place"
import { useProjectSummary } from "@/lib/project-name"
import { mobileNav, palette } from "@/lib/stores"
import { cn } from "@/lib/utils"
import { projectTitle } from "@/model/project"

const iconButton =
  "flex size-8 items-center justify-center rounded-lg text-muted-foreground outline-none hover:bg-muted hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring"

const crumbLink =
  "min-w-0 truncate rounded-md px-1 outline-none hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring"

export function TopBar() {
  const t = useT()
  const place = usePlace()
  // The service crumb belongs to the service page; the Architecture quick look keeps the project's.
  const serviceKey = place.entry ? undefined : place.service
  const [kind, name] = serviceKey?.split("/") ?? []
  const project = useProjectSummary(place.project ?? "")

  let crumbs: ReactNode
  if (place.project) {
    crumbs = (
      <>
        <Link to="/cluster" className={cn(crumbLink, "hidden sm:inline")}>
          {t("shell.cluster")}
        </Link>
        <Slash className="hidden sm:inline" />
        <Link
          to="/projects/$project"
          params={{ project: place.project }}
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
        <Link to="/cluster" className={crumbLink}>
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
    <header className="flex h-14 shrink-0 items-center gap-2 border-b bg-background px-3 md:px-6">
      <button
        type="button"
        className={cn(iconButton, "md:hidden")}
        onClick={() => mobileNav.set(true)}
        aria-label={t("shell.menu")}
      >
        <ListIcon className="size-5" aria-hidden="true" />
      </button>
      <nav
        aria-label={t("shell.breadcrumb")}
        className="flex min-w-0 flex-1 items-center gap-1 text-sm text-muted-foreground"
      >
        {crumbs}
      </nav>
      <AdminBadge />
      <button
        type="button"
        onClick={() => palette.set(true)}
        className="hidden h-8 w-55 items-center gap-2 rounded-lg border bg-background px-2.5 text-sm text-muted-foreground outline-none hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring lg:flex"
      >
        <MagnifyingGlassIcon className="size-4" aria-hidden="true" />
        <span className="flex-1 text-left">{t("shell.jumpTo")}</span>
        <kbd className="font-sans type-micro">⌘K</kbd>
      </button>
      <button
        type="button"
        onClick={() => palette.set(true)}
        className={cn(iconButton, "lg:hidden")}
        aria-label={t("shell.search")}
      >
        <MagnifyingGlassIcon className="size-4" aria-hidden="true" />
      </button>
    </header>
  )
}

function Slash({ className }: { className?: string }) {
  return (
    <span aria-hidden="true" className={cn("px-0.5 text-muted-foreground/60", className)}>
      /
    </span>
  )
}
