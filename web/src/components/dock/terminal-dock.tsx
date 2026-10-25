import { useQuery } from "@tanstack/react-query"
import { useEffect, useRef, useState } from "react"
import CaretDownIcon from "~icons/ph/caret-down"
import CaretUpIcon from "~icons/ph/caret-up"
import TerminalWindowIcon from "~icons/ph/terminal-window"

import { projectQuery, toolsQuery } from "@/api/queries"
import { NodeShellTab } from "@/components/dock/node-shell-tab"
import { KubectlTab, ShellTab, ToolsPods } from "@/components/dock/tabs"
import { useT } from "@/i18n"
import {
  closeTab,
  dock,
  dockHeight,
  dockMinHeight,
  dockShortcut,
  openKubectl,
  openShell,
  selectTab,
  showDock,
  type DockTab as Tab,
} from "@/lib/dock"
import { usePlace } from "@/lib/place"
import { isTask, serviceId } from "@/model/service"
import { Button } from "@zeabur/design-system/components/button"
import {
  Dock,
  DockBar,
  DockHeader,
  DockPanel,
  DockResizer,
  DockTab,
  DockTabList,
  DockTabs,
} from "@zeabur/design-system/components/dock"
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyTitle,
} from "@zeabur/design-system/components/empty"
import { Kbd } from "@zeabur/design-system/components/kbd"
import { usePresence } from "@zeabur/design-system/components/presence"
import { Tooltip } from "@zeabur/design-system/components/tooltip"

/** Room left above the dock for the page's header. */
const pageRoom = 160

const maxHeight = () => Math.max(dockMinHeight, window.innerHeight - pageRoom)

export function TerminalDock() {
  const t = useT()
  const state = dock.useValue()
  const height = dockHeight.useValue()
  // Mounted closed, so the first opening moves as every later one does.
  const [mounted, setMounted] = useState(false)
  useEffect(() => setMounted(true), [])
  const presence = usePresence<HTMLElement>(state.open && mounted)
  const [max, setMax] = useState(maxHeight)
  const bar = useRef<HTMLButtonElement>(null)
  const section = presence.props.ref

  useEffect(() => {
    const onResize = () => setMax(maxHeight())
    window.addEventListener("resize", onResize)
    return () => window.removeEventListener("resize", onResize)
  }, [])

  // Escape inside the terminal is the shell's (vi, less); elsewhere in the dock it folds it. A
  // native listener, so an Escape that closes a dialog or a select opened from the dock stays
  // theirs.
  useEffect(() => {
    const element = section.current
    if (!element) return
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.defaultPrevented) return
      if (event.target instanceof Element && event.target.closest(".xterm")) return
      showDock(false)
    }
    element.addEventListener("keydown", onKey)
    return () => element.removeEventListener("keydown", onKey)
  }, [section])

  // Folding it from inside moves the keyboard to the bar, not to the top of the page.
  const wasOpen = useRef(state.open)
  useEffect(() => {
    if (wasOpen.current && !state.open && section.current?.contains(document.activeElement))
      bar.current?.focus()
    wasOpen.current = state.open
  }, [state.open, section])

  const active = state.tabs.some((tab) => tab.id === state.active)
    ? state.active
    : state.tabs[0]?.id
  const shown = Math.min(height, max)
  const hide = (
    <Tooltip
      content={
        <span className="flex items-center gap-2">
          {t("dock.hide")}
          <Kbd keys={dockShortcut} />
        </span>
      }
    >
      <Button
        variant="muted"
        size="icon-xs"
        aria-label={t("dock.hide")}
        aria-keyshortcuts="Control+`"
        aria-controls="dock"
        aria-expanded
        onClick={() => showDock(false)}
      >
        <CaretDownIcon aria-hidden="true" />
      </Button>
    </Tooltip>
  )

  return (
    <>
      <Dock
        {...presence.props}
        id="dock"
        hidden={!presence.mounted}
        aria-label={t("dock.label")}
        style={{ "--dock-height": `${shown}px` }}
      >
        <DockResizer
          aria-label={t("dock.resize")}
          value={shown}
          min={dockMinHeight}
          max={max}
          onValueChange={dockHeight.set}
        />
        {state.tabs.length === 0 ? (
          <>
            <DockHeader>
              <span className="flex min-w-0 flex-1 items-center gap-1.5 px-1 type-body text-secondary">
                <TerminalWindowIcon aria-hidden="true" className="size-3.5 shrink-0" />
                {t("dock.label")}
              </span>
              {hide}
            </DockHeader>
            <NoTerminals />
          </>
        ) : (
          <DockTabs value={active} onValueChange={(value) => selectTab(String(value))}>
            <DockHeader>
              <DockTabList aria-label={t("dock.label")}>
                {state.tabs.map((tab) => (
                  <DockTab
                    key={tab.id}
                    value={tab.id}
                    closeLabel={t("dock.close", tabName(tab, t("clusterTools.tab")))}
                    onClose={() => closeTab(tab.id)}
                  >
                    <TerminalWindowIcon aria-hidden="true" />
                    <TabLabel tab={tab} />
                  </DockTab>
                ))}
              </DockTabList>
              {hide}
            </DockHeader>
            {state.tabs.map((tab) => {
              const on = state.open && tab.id === active
              return (
                <DockPanel key={tab.id} value={tab.id}>
                  {tab.kind === "shell" ? (
                    <ShellTab tab={tab} shown={on} focus={state.focus} />
                  ) : tab.kind === "node" ? (
                    <NodeShellTab tab={tab} shown={on} focus={state.focus} />
                  ) : (
                    <KubectlTab tab={tab} shown={on} focus={state.focus} />
                  )}
                </DockPanel>
              )
            })}
          </DockTabs>
        )}
      </Dock>
      {!state.open && state.tabs.length > 0 ? (
        <DockBar
          ref={bar}
          aria-controls="dock"
          aria-expanded={false}
          aria-keyshortcuts="Control+`"
          onClick={() => showDock(true)}
        >
          <TerminalWindowIcon aria-hidden="true" />
          <span>{t("dock.count", state.tabs.length)}</span>
          <span className="ml-auto flex items-center gap-2">
            <Kbd keys={dockShortcut} className="max-md:hidden" />
            <CaretUpIcon aria-hidden="true" />
          </span>
        </DockBar>
      ) : null}
    </>
  )
}

function tabParts(tab: Tab, cluster: string): [string, string | undefined] {
  if (tab.kind === "node") return [tab.node, "root"]
  if (tab.kind === "kubectl") return ["kubectl", tab.project ?? cluster]
  return [tab.service.name, tab.container]
}

function tabName(tab: Tab, cluster: string): string {
  if (tab.kind === "node") return `root@${tab.node}`
  return tabParts(tab, cluster).filter(Boolean).join(" ")
}

function TabLabel({ tab }: { tab: Tab }) {
  const t = useT()
  const [main, detail] = tabParts(tab, t("clusterTools.tab"))
  return (
    <span className="flex min-w-0 items-baseline gap-1.5" translate="no">
      <span className="truncate">{main}</span>
      {detail ? (
        <>
          {" "}
          <span className="truncate text-tertiary">{detail}</span>
        </>
      ) : null}
    </span>
  )
}

/** Nothing open: what can be, from where the person is. */
function NoTerminals() {
  const t = useT()
  const place = usePlace()
  const project = place.project
  const tools = useQuery({
    ...toolsQuery(project ?? ""),
    enabled: !!project,
    refetchInterval: (query) => {
      const pods = query.state.data?.pods ?? []
      return pods.some((p) => !p.ready) ? 1500 : pods.length > 0 ? 15_000 : false
    },
  })
  const view = useQuery({ ...projectQuery(project ?? "", { poll: false }), enabled: !!project })
  const service = view.data?.services.find((s) => serviceId(s) === place.service)
  const pods = tools.data?.pods ?? []
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-4">
      <Empty size="inline">
        <EmptyHeader>
          <EmptyTitle>{t("dock.emptyTitle")}</EmptyTitle>
          <EmptyDescription>{t("dock.emptyBody")}</EmptyDescription>
        </EmptyHeader>
        {project && (service || tools.data?.available) ? (
          <div className="flex flex-wrap gap-2">
            {project && service && !isTask(service.kind) ? (
              <Button size="sm" variant="outline" onClick={() => openShell(project, service)}>
                {t("palette.openTerminal", service.name)}
              </Button>
            ) : null}
            {project && tools.data?.available ? (
              <Button size="sm" variant="outline" onClick={() => openKubectl(project)}>
                {t("action.openToolsTerminal")}
              </Button>
            ) : null}
          </div>
        ) : null}
      </Empty>
      {project && tools.data?.available && pods.length > 0 ? (
        <ToolsPods project={project} pods={pods} />
      ) : null}
    </div>
  )
}
