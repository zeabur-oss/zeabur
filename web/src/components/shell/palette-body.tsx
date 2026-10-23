// https://base-ui.com/react/components/autocomplete#command-palette
// oxlint-disable-next-line no-restricted-imports -- the design system has no inline autocomplete (command list) yet
import { Autocomplete } from "@base-ui/react/autocomplete"
import { useQueries, useQuery } from "@tanstack/react-query"
import { useRouter } from "@tanstack/react-router"
import { useState, type ReactNode } from "react"
import ArrowRightIcon from "~icons/ph/arrow-right"
import ClockCounterClockwiseIcon from "~icons/ph/clock-counter-clockwise"
import CubeIcon from "~icons/ph/cube"
import DatabaseIcon from "~icons/ph/database"
import FileTextIcon from "~icons/ph/file-text"
import FolderSimpleIcon from "~icons/ph/folder-simple"
import GitBranchIcon from "~icons/ph/git-branch"
import GlobeIcon from "~icons/ph/globe"
import MagnifyingGlassIcon from "~icons/ph/magnifying-glass"
import MoonIcon from "~icons/ph/moon"
import PackageIcon from "~icons/ph/package"
import PlusIcon from "~icons/ph/plus"
import PuzzlePieceIcon from "~icons/ph/puzzle-piece"
import SlidersHorizontalIcon from "~icons/ph/sliders-horizontal"
import TerminalWindowIcon from "~icons/ph/terminal-window"
import TextAlignLeftIcon from "~icons/ph/text-align-left"
import TranslateIcon from "~icons/ph/translate"

import { logsReadable } from "@/api/logs-access"
import { namespaceQuery, projectQuery, projectsQuery, toolsQuery } from "@/api/queries"
import { useAddChoices } from "@/components/add/menu"
import { ServiceIcon } from "@/components/service-icon"
import { StatusDot } from "@/components/status"
import { localeNames, locales, useI18n, useT } from "@/i18n"
import type { Icon } from "@/lib/icon"
import { addressOf, objectKey } from "@/lib/objects"
import { openAdd } from "@/lib/adding"
import { dock, openKubectl, openShell, toggleDock } from "@/lib/dock"
import { serviceLink, usePlace } from "@/lib/place"
import { palette, recent, remember } from "@/lib/stores"
import { cn } from "@/lib/utils"
import { engineName } from "@/model/databases"
import { domainsOf, isTask, serviceId, statusOf, type Service } from "@/model/service"
import { cycleTheme } from "@/theme"

interface Item {
  id: string
  group: string
  label: string
  hint?: string
  /** Words it also answers to in any language, such as a verb's English name. */
  keywords?: string
  icon?: Icon
  node?: ReactNode
  href?: string
  run?: () => void
}

interface Group {
  label: string
  items: Item[]
}

export function PaletteBody() {
  const t = useT()
  const router = useRouter()
  const place = usePlace()
  const href = (...link: Parameters<typeof serviceLink>) =>
    router.buildLocation(serviceLink(...link)).href
  const { locale, setLocale } = useI18n()
  const [query, setQuery] = useState("")
  const projects = useQuery(projectsQuery)
  const adopted = (projects.data?.items ?? []).filter((p) => p.adopted)
  // Services come from the list. A project's pods, domains and objects need its document: the
  // one on screen always, the others once a query of two letters or more could name them. Not
  // polled: polling every project would multiply the cluster reads.
  const deep = query.trim().length >= 2
  const views = useQueries({
    queries: adopted.map((p) => ({
      ...projectQuery(p.name, { poll: false }),
      enabled: deep || p.name === place.project,
    })),
  })
  const namespaces = useQueries({
    queries: adopted.map((p) => ({
      ...namespaceQuery(p.name),
      enabled: deep || p.name === place.project,
    })),
  })
  const logs = (project: string) =>
    logsReadable(namespaces.find((n) => n.data?.name === project)?.data)
  const recents = recent.useValue()
  const tools = useQuery({ ...toolsQuery(place.project ?? ""), enabled: !!place.project })
  const adds = useAddChoices(place.project)

  const items: Item[] = []
  const g = {
    recent: t("palette.recent"),
    projects: t("shell.projects"),
    services: t("project.services"),
    pods: t("pod.pods"),
    domains: t("common.domains"),
    objects: t("project.entry.objects"),
    actions: t("palette.actions"),
  }
  if (!query)
    for (const r of recents)
      items.push({
        id: `recent:${r.id}`,
        group: g.recent,
        label: r.label,
        hint: r.hint,
        icon: ClockCounterClockwiseIcon,
        href: r.href,
      })

  const currentKey = place.service
  const currentProject = views.find((v) => v.data?.namespace === place.project)?.data
  const current = currentProject?.services.find((s) => serviceId(s) === currentKey)
  // Verbs for the service on screen; for the others, they show once the query names them.
  const verbs = (project: string, s: Service, prefix: string) => [
    ...(logs(project)
      ? [
          {
            id: `${prefix}:logs`,
            keywords: `logs log ${s.name}`,
            group: g.actions,
            label: t("palette.openLogs", s.name),
            icon: TextAlignLeftIcon,
            href: href(project, s, { tab: "logs" }),
          },
        ]
      : []),
    ...(isTask(s.kind)
      ? []
      : [
          {
            id: `${prefix}:terminal`,
            keywords: `terminal shell exec ${s.name}`,
            group: g.actions,
            label: t("palette.openTerminal", s.name),
            icon: TerminalWindowIcon,
            run: () => openShell(project, s),
          },
        ]),
    {
      id: `${prefix}:variables`,
      keywords: `variables env ${s.name}`,
      group: g.actions,
      label: t("palette.openVariables", s.name),
      icon: SlidersHorizontalIcon,
      href: href(project, s, { tab: "variables" }),
    },
    {
      id: `${prefix}:changes`,
      keywords: `deployments changes releases history rollback ${s.name}`,
      group: g.actions,
      label: t("palette.openChanges", s.name),
      icon: ClockCounterClockwiseIcon,
      href: href(project, s, { tab: "deployments" }),
    },
  ]
  if (current && place.project) items.push(...verbs(place.project, current, "action"))
  const project = place.project
  if (project && tools.data?.available)
    items.push({
      id: "action:tools",
      keywords: "kubectl helm terminal shell",
      group: g.actions,
      label: t("action.openToolsTerminal"),
      icon: TerminalWindowIcon,
      run: () => openKubectl(project),
    })
  if (project && adds?.image)
    items.push({
      id: "action:add-image",
      keywords: "docker container deploy service new",
      group: g.actions,
      label: t("add.palette.image", project),
      icon: PackageIcon,
      run: () => openAdd(project, "image"),
    })
  if (project && adds?.git.open)
    items.push({
      id: "action:add-git",
      keywords: "git github gitlab repository build deploy service new",
      group: g.actions,
      label: t("add.palette.git", project),
      icon: GitBranchIcon,
      run: () => openAdd(project, "git"),
    })
  for (const { engine } of adds?.databases.filter((d) => d.installed) ?? [])
    if (project)
      items.push({
        id: `action:add-${engine}`,
        keywords: "database new",
        group: g.actions,
        label: t("add.palette.database", engineName(engine), project),
        icon: DatabaseIcon,
        run: () => openAdd(project, engine),
      })
  if (project)
    items.push({
      id: "action:create",
      keywords: "yaml new object manifest",
      group: g.actions,
      label: t("create.palette", project),
      icon: PlusIcon,
      href: router.buildLocation({ to: "/projects/$project/create", params: { project } }).href,
    })

  for (const p of projects.data?.items ?? [])
    items.push({
      id: `project:${p.name}`,
      group: g.projects,
      label: p.name,
      hint: p.adopted ? t("projects.adopted") : t("projects.namespaceOnly"),
      icon: FolderSimpleIcon,
      href: router.buildLocation({ to: "/projects/$project", params: { project: p.name } }).href,
    })

  for (const [i, summary] of adopted.entries()) {
    const data = views[i]?.data
    if (!data) {
      for (const s of summary.services ?? [])
        items.push({
          id: `service:${summary.name}/${serviceId(s)}`,
          group: g.services,
          label: s.name,
          hint: t("palette.in", s.kind, summary.name),
          node: (
            <span className="flex items-center gap-2">
              <span translate="no">{s.name}</span>
              <StatusDot health={s.health} />
            </span>
          ),
          href: href(summary.name, s),
        })
      continue
    }
    for (const s of data.services) {
      const serviceHref = href(data.namespace, s)
      items.push({
        id: `service:${data.namespace}/${serviceId(s)}`,
        group: g.services,
        label: s.name,
        hint: t("palette.in", s.kind, data.namespace),
        node: (
          <span className="flex items-center gap-2">
            <ServiceIcon service={s} size="sm" />
            <span translate="no">{s.name}</span>
            <StatusDot health={statusOf(s).health} />
          </span>
        ),
        href: serviceHref,
      })
      if (query && s !== current)
        items.push(...verbs(data.namespace, s, `verb:${data.namespace}/${serviceId(s)}`))
      for (const pod of s.pods)
        items.push({
          id: `pod:${data.namespace}/${pod.name}`,
          group: g.pods,
          label: pod.name,
          hint: `${t("palette.in", s.name, data.namespace)}${pod.ready ? "" : `${t("common.sep")}${pod.reason ?? t("pod.notReady")}`}`,
          icon: CubeIcon,
          href: href(data.namespace, s, {
            tab: logs(data.namespace) ? "logs" : "events",
            pod: pod.name,
          }),
        })
      for (const { host } of domainsOf(data, s))
        items.push({
          id: `domain:${host}`,
          group: g.domains,
          label: host,
          hint: t("palette.in", s.name, data.namespace),
          icon: GlobeIcon,
          href: href(data.namespace, s, { tab: "networking" }),
        })
    }
    for (const r of data.resources) {
      const address = addressOf(r.kind, r.name)
      if (!address) continue
      items.push({
        id: `object:${data.namespace}/${r.kind}/${r.name}`,
        group: g.objects,
        label: r.name,
        hint: t("palette.in", r.kind, data.namespace),
        icon: FileTextIcon,
        href: router.buildLocation({
          to: "/projects/$project/objects",
          params: { project: data.namespace },
          search: { object: objectKey(address) },
        }).href,
      })
    }
  }

  items.push({
    id: "action:dock",
    keywords: "terminal shell dock console",
    group: g.actions,
    label: dock.get().open ? t("dock.hide") : t("dock.show"),
    icon: TerminalWindowIcon,
    run: toggleDock,
  })
  items.push({
    id: "action:theme",
    group: g.actions,
    label: t("palette.toggleTheme"),
    icon: MoonIcon,
    run: cycleTheme,
  })
  items.push({
    id: "action:components",
    group: g.actions,
    label: t("palette.components"),
    icon: PuzzlePieceIcon,
    href: "/cluster/components",
  })
  for (const value of locales)
    if (value !== locale)
      items.push({
        id: `action:locale:${value}`,
        group: g.actions,
        label: `${t("settings.language")}${t("common.colon")}${localeNames[value]}`,
        icon: TranslateIcon,
        run: () => setLocale(value),
      })

  const needle = query.trim().toLowerCase()
  const shown = needle
    ? items
        .filter((item) => {
          const text = `${item.label} ${item.hint ?? ""} ${item.keywords ?? ""}`.toLowerCase()
          return needle.split(/\s+/).every((word) => text.includes(word))
        })
        .slice(0, 40)
    : items
        .filter(
          (item) => item.group !== g.pods && item.group !== g.domains && item.group !== g.objects,
        )
        .slice(0, 30)

  const choose = (item: Item | undefined) => {
    if (!item) return
    palette.set(false)
    if (item.href) {
      if (!item.id.startsWith("action:"))
        remember({
          id: item.id.replace(/^recent:/, ""),
          label: item.label,
          hint: item.hint ?? "",
          href: item.href,
        })
      router.history.push(item.href)
    }
    item.run?.()
  }

  const groups: Group[] = []
  for (const item of shown) {
    const group = groups.find((g) => g.label === item.group)
    if (group) group.items.push(item)
    else groups.push({ label: item.group, items: [item] })
  }

  return (
    <Autocomplete.Root
      open
      inline
      items={groups}
      filter={null}
      value={query}
      onValueChange={setQuery}
      itemToStringValue={(item: Item) => item.label}
      autoHighlight="always"
      keepHighlight
    >
      <div className="flex items-center gap-2.5 border-b px-4">
        <MagnifyingGlassIcon className="size-4.5 text-muted-foreground" aria-hidden="true" />
        <Autocomplete.Input
          // oxlint-disable-next-line jsx-a11y/no-autofocus -- the palette exists to be typed into
          autoFocus
          aria-label={t("palette.search")}
          className="h-13 w-full bg-transparent type-heading font-normal outline-none placeholder:text-muted-foreground"
        />
        <kbd className="rounded border bg-muted px-1.5 type-nano leading-normal text-muted-foreground">
          esc
        </kbd>
      </div>
      <Autocomplete.Empty className="px-3 py-6 text-center text-sm text-muted-foreground empty:hidden">
        {t("palette.empty")}
      </Autocomplete.Empty>
      <Autocomplete.List className="flex max-h-palette flex-col overflow-y-auto p-1.5 empty:hidden">
        {(group: Group) => (
          <Autocomplete.Group key={group.label} items={group.items}>
            <Autocomplete.GroupLabel className="px-2.5 pt-2.5 pb-1 type-meta font-medium text-muted-foreground">
              {group.label}
            </Autocomplete.GroupLabel>
            <Autocomplete.Collection>
              {(item: Item) => {
                const ItemIcon = item.icon
                return (
                  <Autocomplete.Item
                    key={item.id}
                    value={item}
                    onClick={() => choose(item)}
                    className="group flex h-9 cursor-default items-center gap-2.5 rounded-lg px-2.5 text-sm outline-none data-highlighted:bg-selected"
                  >
                    {item.node ?? (
                      <>
                        {ItemIcon ? (
                          <ItemIcon className="size-4 text-muted-foreground" aria-hidden="true" />
                        ) : null}
                        <span className="truncate" translate="no">
                          {item.label}
                        </span>
                      </>
                    )}
                    {item.hint ? (
                      <span className="ml-auto truncate pl-3 type-micro leading-4 text-muted-foreground">
                        {item.hint}
                      </span>
                    ) : null}
                    <ArrowRightIcon
                      className={cn(
                        "hidden size-3.5 text-muted-foreground group-data-highlighted:block",
                        !item.hint && "ml-auto",
                      )}
                      aria-hidden="true"
                    />
                  </Autocomplete.Item>
                )
              }}
            </Autocomplete.Collection>
          </Autocomplete.Group>
        )}
      </Autocomplete.List>
      <div className="flex items-center gap-3 border-t bg-muted/40 px-4 py-2 type-micro leading-normal text-muted-foreground">
        <span>↑↓ {t("palette.move")}</span>
        <span>↵ {t("palette.open")}</span>
      </div>
    </Autocomplete.Root>
  )
}
