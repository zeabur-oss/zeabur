// The page of something an operator runs, a database or object storage, with a service's tabs.
import { useQuery } from "@tanstack/react-query"
import { useNavigate } from "@tanstack/react-router"
import type { ReactNode } from "react"
import { toast } from "sonner"
import CopyIcon from "~icons/ph/copy"
import DotsThreeIcon from "~icons/ph/dots-three"
import TrashIcon from "~icons/ph/trash"

import { useLogsReadable } from "@/api/logs-access"
import { projectQuery } from "@/api/queries"
import { GatedMenuItem, useGates } from "@/components/gate"
import { usePlanner } from "@/components/plan/planner"

import { Freshness } from "@/components/freshness"
import { useI18n, useT } from "@/i18n"
import type { Icon } from "@/lib/icon"
import { formatDate } from "@/lib/format"
import { cn } from "@/lib/utils"
import type { Gate } from "@/model/rights"
import {
  modeSearch,
  podSearch,
  tabSearch,
  type DatabaseTab,
  type ObjectStorageTab,
  type ServiceMode,
} from "@/routes/search"
import { TabRow } from "@/routes/service/tab-row"
import { Button } from "@zeabur/design-system/components/button"
import {
  Menu,
  MenuContent,
  MenuItem,
  MenuSeparator,
  MenuTrigger,
} from "@zeabur/design-system/components/menu"
import { PageColumn } from "@zeabur/design-system/components/page-column"
import { Tabs, TabsList, TabsTrigger } from "@zeabur/design-system/components/tabs"

export function StoreShell<Tab extends string>({
  project,
  icon: StoreIcon,
  name,
  state,
  kind,
  created,
  updatedAt,
  failed,
  actions,
  tab,
  tabs,
  tabsLabel,
  onTab,
  children,
}: {
  project: string
  icon: Icon
  name: string
  state: ReactNode
  /** The engine and version, under the name. */
  kind: string
  created?: string | null
  updatedAt: number
  failed: boolean
  actions: ReactNode
  tab: Tab
  /** Each tab shown, with its icon and label. */
  tabs: { value: Tab; icon: Icon; label: string }[]
  tabsLabel: string
  onTab: (tab: Tab) => void
  children: ReactNode
}) {
  const t = useT()
  const { locale } = useI18n()
  const wide = tab === "logs"
  return (
    <Tabs
      value={tab}
      onValueChange={(value) => onTab(value as Tab)}
      variant="page"
      className={cn("flex flex-1 flex-col", wide && "min-h-0")}
    >
      <header className="shrink-0">
        <PageColumn width="content">
          <div className="flex flex-wrap items-start gap-x-3 gap-y-3 py-4 md:py-6">
            <span className="flex size-10 shrink-0 items-center justify-center rounded-xl border border-main bg-card">
              <StoreIcon aria-hidden="true" className="size-5" />
            </span>
            <div className="flex min-w-0 flex-1 flex-col gap-1">
              <div className="flex min-w-0 flex-wrap items-center gap-x-2.5 gap-y-1">
                <h1 className="min-w-0 truncate type-name" translate="no">
                  {name}
                </h1>
                {state}
              </div>
              <div className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-0.5 text-xs text-secondary">
                <span className="whitespace-nowrap">{kind}</span>
                {created ? (
                  <span className="whitespace-nowrap">
                    {t("service.created", formatDate(created, locale))}
                  </span>
                ) : null}
                <Freshness project={project} updatedAt={updatedAt} failed={failed} />
              </div>
            </div>
            <div className="flex shrink-0 items-center gap-1.5 max-sm:w-full max-sm:pl-13">
              {actions}
            </div>
          </div>
          <TabRow>
            <TabsList aria-label={tabsLabel} className="w-full">
              {tabs.map(({ value, icon: TabIcon, label }) => (
                <TabsTrigger key={value} value={value} className="flex-1">
                  <TabIcon aria-hidden="true" className="max-sm:hidden" />
                  {label}
                </TabsTrigger>
              ))}
            </TabsList>
          </TabRow>
        </PageColumn>
      </header>
      <PageColumn width="content" className={cn("mt-3", wide ? "mb-4 min-h-0 flex-1" : "mb-16")}>
        {children}
      </PageColumn>
    </Tabs>
  )
}

/** What both store pages read: the tab shown, who may read logs, the person's gates, unreadable
 * kinds, and a planner that goes back to the project after a delete. */
export function useStorePage<Tab extends DatabaseTab | ObjectStorageTab>(
  project: string,
  asked: Tab | undefined,
  all: readonly Tab[],
) {
  const leave = useNavigate()
  const go = useNavigate()
  const logs = useLogsReadable(project)
  return {
    setTab: (next: Tab) => void go({ to: ".", search: tabSearch(next), hash: "" }),
    setPod: (pod?: string) => void go({ to: ".", search: podSearch(pod), replace: true }),
    setMode: (mode?: ServiceMode) => void go({ to: ".", search: modeSearch(mode), hash: "" }),
    tab: asked === undefined || (asked === "logs" && !logs) ? ("overview" as Tab) : asked,
    shown: all.filter((value) => value !== "logs" || logs),
    gates: useGates(project),
    unreadable: useQuery(projectQuery(project)).data?.unreadable ?? [],
    deleter: usePlanner(
      project,
      () => void leave({ to: "/projects/$project", params: { project }, search: {} }),
    ),
  }
}

/** The ⋯ menu: copy the object's name, other items, then delete. */
export function StoreMenu({
  copy,
  deleteGate,
  deleteLabel,
  onDelete,
  children,
}: {
  copy: string
  deleteGate: Gate
  deleteLabel: string
  onDelete: () => void
  children?: ReactNode
}) {
  const t = useT()
  const copyName = () =>
    navigator.clipboard.writeText(copy).then(
      () => toast.success(t("common.copied")),
      () => undefined,
    )
  return (
    <Menu>
      <MenuTrigger
        render={<Button variant="outline" size="icon-sm" aria-label={t("service.more")} />}
      >
        <DotsThreeIcon aria-hidden="true" />
      </MenuTrigger>
      <MenuContent align="end" className="w-72">
        <MenuItem onClick={() => void copyName()}>
          <CopyIcon aria-hidden="true" />
          {t("service.copyName")}
        </MenuItem>
        {children}
        <MenuSeparator />
        <GatedMenuItem gate={deleteGate} onClick={onDelete}>
          <TrashIcon aria-hidden="true" />
          {deleteLabel}
        </GatedMenuItem>
      </MenuContent>
    </Menu>
  )
}
