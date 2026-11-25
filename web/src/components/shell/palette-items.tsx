import type { ReactNode } from "react"
import CubeIcon from "~icons/ph/cube"
import FileTextIcon from "~icons/ph/file-text"
import GlobeIcon from "~icons/ph/globe"
import PuzzlePieceIcon from "~icons/ph/puzzle-piece"

import type { Schemas } from "@/api/client"
import { ServiceIcon } from "@/components/service-icon"
import { StatusDot } from "@/components/status"
import type { Translate } from "@/i18n"
import type { Icon } from "@/lib/icon"
import { addressOf, objectKey } from "@/lib/objects"
import type { Answers } from "@/model/pages"
import { domainsOf, serviceId, statusOf, type Service } from "@/model/service"

export interface Item {
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

export interface Group {
  label: string
  items: Item[]
}

/** The group names, translated once. */
export type GroupNames = Record<
  "recent" | "projects" | "services" | "pods" | "domains" | "objects" | "actions",
  string
>

type Summary = Schemas.ProjectList["items"][number]
type Document = Schemas.ProjectDocument

/** The actions that open a page for administrators: only the ones the server says the person may
 * open, the same answers the navigation follows. */
export function pageActions(t: Translate, groups: GroupNames, may: Answers): Item[] {
  return may.components
    ? [
        {
          id: "action:components",
          group: groups.actions,
          label: t("palette.components"),
          icon: PuzzlePieceIcon,
          href: "/cluster/components",
        },
      ]
    : []
}

interface Entries {
  t: Translate
  groups: GroupNames
  href: (...link: Parameters<typeof import("@/lib/place").serviceLink>) => string
  objectHref: (project: string, object: string) => string
  adopted: Summary[]
  /** The project documents, in the order of `adopted`; none while one is not loaded. */
  views: (Document | undefined)[]
  current: Service | undefined
  query: string
  logs: (project: string) => boolean
  verbs: (project: string, service: Service, prefix: string) => Item[]
}

/** The services, pods, domains and objects of the adopted projects. A project whose document is
 * not loaded yet shows the services its summary lists. */
export function entryItems(e: Entries): Item[] {
  return e.adopted.flatMap((summary, i) => {
    const data = e.views[i]
    return data ? loadedItems(e, data) : summaryItems(e, summary)
  })
}

function summaryItems({ t, groups, href }: Entries, summary: Summary): Item[] {
  return (summary.services ?? []).map((s) => ({
    id: `service:${summary.name}/${serviceId(s)}`,
    group: groups.services,
    label: s.name,
    hint: t("palette.in", s.kind, summary.name),
    node: (
      <span className="flex items-center gap-2">
        <span translate="no">{s.name}</span>
        <StatusDot health={s.health} />
      </span>
    ),
    href: href(summary.name, s),
  }))
}

function loadedItems(e: Entries, data: Document): Item[] {
  const { t, groups, href } = e
  const services = data.services.flatMap((s): Item[] => [
    {
      id: `service:${data.namespace}/${serviceId(s)}`,
      group: groups.services,
      label: s.name,
      hint: t("palette.in", s.kind, data.namespace),
      node: (
        <span className="flex items-center gap-2">
          <ServiceIcon service={s} size="sm" />
          <span translate="no">{s.name}</span>
          <StatusDot health={statusOf(s).health} />
        </span>
      ),
      href: href(data.namespace, s),
    },
    ...(e.query && s !== e.current
      ? e.verbs(data.namespace, s, `verb:${data.namespace}/${serviceId(s)}`)
      : []),
    ...s.pods.map((pod): Item => ({
      id: `pod:${data.namespace}/${pod.name}`,
      group: groups.pods,
      label: pod.name,
      hint: `${t("palette.in", s.name, data.namespace)}${pod.ready ? "" : `${t("common.sep")}${pod.reason ?? t("pod.notReady")}`}`,
      icon: CubeIcon,
      href: href(data.namespace, s, {
        tab: e.logs(data.namespace) ? "logs" : "events",
        pod: pod.name,
      }),
    })),
    ...domainsOf(data, s).map(({ host }): Item => ({
      id: `domain:${host}`,
      group: groups.domains,
      label: host,
      hint: t("palette.in", s.name, data.namespace),
      icon: GlobeIcon,
      href: href(data.namespace, s, { tab: "networking" }),
    })),
  ])
  const objects = data.resources.flatMap((r): Item[] => {
    const address = addressOf(r.kind, r.name)
    return address
      ? [
          {
            id: `object:${data.namespace}/${r.kind}/${r.name}`,
            group: groups.objects,
            label: r.name,
            hint: t("palette.in", r.kind, data.namespace),
            icon: FileTextIcon,
            href: e.objectHref(data.namespace, objectKey(address)),
          },
        ]
      : []
  })
  return [...services, ...objects]
}

/** What the palette shows: with a query, every item that holds all its words; without one, the
 * first items, leaving out the groups that only a query reaches. */
export function shownItems(items: Item[], query: string, groups: GroupNames): Item[] {
  const needle = query.trim().toLowerCase()
  if (!needle)
    return items
      .filter(
        (item) =>
          item.group !== groups.pods &&
          item.group !== groups.domains &&
          item.group !== groups.objects,
      )
      .slice(0, 30)
  return items
    .filter((item) => {
      const text = `${item.label} ${item.hint ?? ""} ${item.keywords ?? ""}`.toLowerCase()
      return needle.split(/\s+/).every((word) => text.includes(word))
    })
    .slice(0, 40)
}

export function groupItems(shown: Item[]): Group[] {
  const groups: Group[] = []
  for (const item of shown) {
    const group = groups.find((g) => g.label === item.group)
    if (group) group.items.push(item)
    else groups.push({ label: item.group, items: [item] })
  }
  return groups
}
