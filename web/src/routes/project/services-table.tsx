import { useQuery } from "@tanstack/react-query"
import { getRouteApi, Link, useNavigate } from "@tanstack/react-router"
import { useState } from "react"
import CaretDownIcon from "~icons/ph/caret-down"
import CaretUpIcon from "~icons/ph/caret-up"
import CaretUpDownIcon from "~icons/ph/caret-up-down"
import GlobeIcon from "~icons/ph/globe"

import { Segmented } from "@/components/bits"
import { PodCells, replicaCounts } from "@/components/pods"
import { ServiceIcon } from "@/components/service-icon"
import { ServiceStatus, StatusDot, useStatusWord } from "@/components/status"
import { Button } from "@zeabur/design-system/components/button"
import { Select } from "@zeabur/design-system/components/select"
import { useT } from "@/i18n"
import { usageQuery } from "@/api/queries"
import { Skeleton } from "@zeabur/design-system/components/skeleton"
import { PHONE, useMedia } from "@/lib/media"
import { serviceLink } from "@/lib/place"
import { useQuantityFormat } from "@/lib/quantity"
import { cn } from "@/lib/utils"
import {
  domainsOf,
  healthRank,
  imageTag,
  isHealthy,
  needsAttention,
  restartsOf,
  serviceId,
  statusOf,
  type Project,
  type Service,
} from "@/model/service"
import { serviceUsage } from "@/model/usage"
import { FilterBox } from "@/routes/project/filter-box"
import type { ProjectSearch, SortKey } from "@/routes/search"

const route = getRouteApi("/projects/$project")

/** A select's empty value means none. */
const ALL_KINDS = "*"

const readyRatio = (s: Service) => {
  const { ready, desired } = replicaCounts(s)
  return desired > 0 ? ready / desired : 2
}

function compare(key: SortKey, a: Service, b: Service): number {
  switch (key) {
    case "status":
      return healthRank[statusOf(a).health] - healthRank[statusOf(b).health]
    case "ready": {
      return readyRatio(a) - readyRatio(b)
    }
    case "restarts":
      return restartsOf(a) - restartsOf(b)
    case "kind":
      return a.kind.localeCompare(b.kind)
    default:
      return a.name.localeCompare(b.name)
  }
}

const largestFirst: ReadonlySet<SortKey> = new Set(["restarts"])

export function ServicesTable({
  project,
  data,
  typed,
}: {
  project: string
  data: Project
  typed: string
}) {
  const t = useT()
  const word = useStatusWord()
  const search = route.useSearch()
  const navigate = useNavigate({ from: "/projects/$project" })
  const f = useQuantityFormat()
  const usageRead = useQuery(usageQuery(project))
  const usage = usageRead.data
  // The columns are held while metrics-server answers, so they do not push the rest aside.
  const measuring = usageRead.isPending
  const live = measuring || usage?.state === "available"
  const [query, setQuery] = useState(typed)
  const phone = useMedia(PHONE)
  const set = (patch: Partial<ProjectSearch>) =>
    void navigate({ search: (previous) => ({ ...previous, ...patch }), replace: true })

  const sort = search.sort ?? "status"
  // `descending` is the order shown, which aria-sort states
  // (https://www.w3.org/TR/wai-aria-1.2/#aria-sort).
  const descending = largestFirst.has(sort) !== (search.desc === true)
  const kinds = [...new Set(data.services.map((s) => s.kind))].toSorted()
  const needle = query.trim().toLowerCase()
  const rows = data.services
    .filter((s) => {
      const status = statusOf(s)
      if (search.status === "attention" && !needsAttention(status)) return false
      if (search.status === "healthy" && !isHealthy(status)) return false
      if (search.kind && s.kind !== search.kind) return false
      if (!needle) return true
      return [s.name, s.kind, s.image ?? "", word(status), ...domainsOf(data, s).map((d) => d.host)]
        .join(" ")
        .toLowerCase()
        .includes(needle)
    })
    .toSorted((a, b) => {
      const order = compare(sort, a, b)
      return (descending ? -order : order) || a.name.localeCompare(b.name)
    })
  const attention = data.services.filter((s) => needsAttention(statusOf(s))).length

  const head =
    "px-3 py-2 text-left type-meta font-medium text-muted-foreground whitespace-nowrap shadow-rule"
  const sortable = (key: SortKey, label: string, className?: string) => {
    const active = sort === key
    const SortIcon = active ? (descending ? CaretUpIcon : CaretDownIcon) : CaretUpDownIcon
    return (
      <th
        className={cn(head, className)}
        aria-sort={active ? (descending ? "descending" : "ascending") : undefined}
      >
        <button
          type="button"
          onClick={() =>
            set(active ? { desc: search.desc ? undefined : true } : { sort: key, desc: undefined })
          }
          className="inline-flex items-center gap-1 rounded outline-none hover:text-foreground"
        >
          {label}
          <SortIcon className={cn("size-3", !active && "opacity-40")} aria-hidden="true" />
        </button>
      </th>
    )
  }

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        <FilterBox
          value={query}
          onChange={(value) => {
            setQuery(value)
            set({ q: value || undefined })
          }}
          label={t("services.search")}
          slash
          className="sm:w-64"
        />
        <Segmented
          label={t("table.status")}
          value={search.status ?? "all"}
          onChange={(value) => set({ status: value === "all" ? undefined : value })}
          options={[
            { value: "all", label: t("common.all") },
            {
              value: "attention",
              label: (
                <>
                  {t("services.attention")}
                  <span className="tabular-nums opacity-60">{attention}</span>
                </>
              ),
            },
            { value: "healthy", label: t("services.healthy") },
          ]}
        />
        {phone ? null : (
          <Select
            size="sm"
            value={search.kind ?? ALL_KINDS}
            onValueChange={(kind) => set({ kind: kind === ALL_KINDS ? undefined : kind })}
            aria-label={t("table.kind")}
            items={[
              { value: ALL_KINDS, label: t("services.allKinds") },
              ...kinds.map((kind) => ({ value: kind, label: kind })),
            ]}
          />
        )}
        <span className="ml-auto text-xs text-muted-foreground tabular-nums">
          {t("services.count", rows.length, data.services.length)}
        </span>
      </div>

      {phone ? (
        <ServiceLines project={project} data={data} rows={rows} />
      ) : (
        <div className="relative max-h-frame overflow-auto rounded-xl border border-v2-border bg-card">
          <table className="w-full min-w-180 text-sm" aria-label={t("project.services")}>
            <thead className="sticky top-0 z-1 bg-card">
              <tr>
                {sortable("name", t("table.service"), "pl-4")}
                {sortable("status", t("table.status"))}
                {sortable("ready", t("table.pods"))}
                {sortable("restarts", t("table.restarts"), "text-right")}
                {live ? (
                  <>
                    <th className={cn(head, "w-24 text-right")}>{t("resources.cpu")}</th>
                    <th className={cn(head, "w-24 text-right")}>{t("resources.memory")}</th>
                  </>
                ) : null}
                <th className={head}>{t("table.domain")}</th>
                {sortable("kind", t("table.kind"), "hidden pr-4 2xl:table-cell")}
              </tr>
            </thead>
            <tbody className="divide-y">
              {rows.length === 0 ? (
                <tr>
                  <td
                    colSpan={live ? 8 : 6}
                    className="px-4 py-6 text-center text-sm text-muted-foreground"
                  >
                    {t("services.noMatch")}
                  </td>
                </tr>
              ) : null}
              {rows.map((service) => {
                const status = statusOf(service)
                const restarts = restartsOf(service)
                const domain = domainsOf(data, service)[0]
                const used = live ? serviceUsage(service, usage) : undefined
                return (
                  <tr key={serviceId(service)} className="hover:bg-hover">
                    <td className="max-w-64 py-2 pr-3 pl-4">
                      <Link
                        {...serviceLink(project, service)}
                        className="flex items-center gap-2.5 rounded-md outline-none"
                        title={service.name}
                      >
                        <ServiceIcon service={service} size="sm" />
                        <span className="flex min-w-0 flex-col leading-tight">
                          <span className="truncate font-medium hover:underline" translate="no">
                            {service.name}
                          </span>
                          {service.image ? (
                            <span
                              className="truncate font-mono type-micro leading-tight text-muted-foreground"
                              translate="no"
                            >
                              {imageTag(service.image)}
                            </span>
                          ) : null}
                        </span>
                      </Link>
                    </td>
                    <td className="px-3 py-2">
                      <ServiceStatus status={status} size="sm" />
                    </td>
                    <td className="px-3 py-2">
                      <PodCells service={service} project={project} />
                    </td>
                    <td
                      className={cn(
                        "px-3 py-2 text-right tabular-nums",
                        restarts === 0 && "text-muted-foreground",
                        status.health === "fail" && restarts > 0 && "font-medium text-fail",
                      )}
                    >
                      {restarts}
                    </td>
                    {live ? (
                      <>
                        <td className="px-3 py-2 text-right tabular-nums">
                          {measuring ? (
                            <Skeleton className="ml-auto h-4 w-10" />
                          ) : used?.cpu !== undefined ? (
                            f.cpu(used.cpu)
                          ) : (
                            "—"
                          )}
                        </td>
                        <td
                          className={cn(
                            "px-3 py-2 text-right tabular-nums",
                            used?.nearLimit && "font-medium text-warn-strong",
                          )}
                        >
                          {measuring ? (
                            <Skeleton className="ml-auto h-4 w-10" />
                          ) : used?.memory !== undefined ? (
                            f.memory(used.memory)
                          ) : (
                            "—"
                          )}
                          {used?.nearLimit ? (
                            <span className="sr-only">{` ${t("usage.note.nearLimit")}`}</span>
                          ) : null}
                        </td>
                      </>
                    ) : null}
                    <td className="max-w-52 px-3 py-2">
                      {domain ? (
                        <span className="flex items-center gap-1.5 text-xs">
                          <GlobeIcon
                            className="size-3.5 shrink-0 text-muted-foreground"
                            aria-hidden="true"
                          />
                          <span className="truncate font-mono" translate="no">
                            {domain.host}
                          </span>
                        </span>
                      ) : (
                        <span className="text-xs text-muted-foreground">—</span>
                      )}
                    </td>
                    <td
                      className="hidden py-2 pr-4 pl-3 text-xs text-muted-foreground 2xl:table-cell"
                      translate="no"
                    >
                      {service.kind}
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        </div>
      )}
    </div>
  )
}

/** Rows a phone lists past the ones that need attention, before "Show all". */
const PHONE_ROWS = 10

/** On a phone, one line per service: its status, name, status word and one fact. */
function ServiceLines({
  project,
  data,
  rows,
}: {
  project: string
  data: Project
  rows: Service[]
}) {
  const t = useT()
  const word = useStatusWord()
  const [all, setAll] = useState(false)
  const trouble = rows.filter((s) => needsAttention(statusOf(s))).length
  const limit = trouble + PHONE_ROWS
  const shown = all || rows.length <= limit + 1 ? rows : rows.slice(0, limit)
  if (rows.length === 0)
    return <p className="py-4 text-center text-sm text-muted-foreground">{t("services.noMatch")}</p>
  return (
    <div className="flex flex-col gap-1">
      <ul
        aria-label={t("project.services")}
        className="flex flex-col rounded-xl border border-v2-border bg-card"
      >
        {shown.map((service) => {
          const status = statusOf(service)
          const domain = domainsOf(data, service)[0]
          const fact =
            needsAttention(status) || !domain
              ? status.desired !== undefined
                ? t("attention.ready", status.ready ?? 0, status.desired)
                : undefined
              : domain.host
          return (
            <li key={serviceId(service)} className="border-t border-rule first:border-t-0">
              <Link
                {...serviceLink(project, service)}
                className="flex min-w-0 items-center gap-2.5 px-3 py-2.5 outline-none hover:bg-hover"
              >
                <StatusDot health={status.health} />
                <span className="min-w-0 flex-1 truncate font-medium" translate="no">
                  {service.name}
                </span>
                <span
                  className={cn(
                    "shrink-0 type-meta",
                    status.health === "fail" ? "text-fail" : "text-muted-foreground",
                  )}
                >
                  {word(status)}
                </span>
                {fact ? (
                  <span
                    className="max-w-28 shrink truncate type-meta text-muted-foreground tabular-nums"
                    translate="no"
                  >
                    {fact}
                  </span>
                ) : null}
              </Link>
            </li>
          )
        })}
      </ul>
      {shown.length < rows.length || all ? (
        <Button
          variant="ghost"
          size="xs"
          className="-ml-2.5 self-start"
          onClick={() => setAll(!all)}
        >
          {all ? t("services.showFewer") : t("services.showAll", rows.length)}
        </Button>
      ) : null}
    </div>
  )
}
