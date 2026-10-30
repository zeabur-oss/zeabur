// The project's facts beside its services: domains, storage, databases, backups and what happened
// lately.
import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { useState, type ReactNode } from "react"
import ArrowRightIcon from "~icons/ph/arrow-right"
import CaretRightIcon from "~icons/ph/caret-right"
import GlobeIcon from "~icons/ph/globe"
import LockSimpleIcon from "~icons/ph/lock-simple"
import LockSimpleOpenIcon from "~icons/ph/lock-simple-open"

import type { Schemas } from "@/api/client"
import { projectAlertsQuery, projectQuery, projectTimelineQuery } from "@/api/queries"
import { AlertList, AlertsRefused, RecentNote } from "@/components/alerts"
import { Ago, Segmented } from "@/components/bits"
import { refusedAmong, RefusedKinds } from "@/components/refused"
import { StatusDot, StatusPill } from "@/components/status"
import { Button } from "@zeabur/design-system/components/button"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur/design-system/components/collapsible"
import { Drawer } from "@zeabur/design-system/components/drawer"
import { Timeline, TimelineItem } from "@zeabur/design-system/components/timeline"
import { Tooltip } from "@zeabur/design-system/components/tooltip"
import {
  BackupDetails,
  CoverageRow,
  ProviderNotes,
  shownCoverage,
  useBackups,
} from "@/components/backups"
import {
  NewObjectStorageButton,
  ObjectStorageNotes,
  ObjectStorageRow,
  useObjectStorage,
} from "@/components/object-storage"
import {
  DatabaseRow,
  installedEngines,
  NewDatabaseButton,
  OperatorNotes,
  useDatabases,
} from "@/components/databases"
import { storageRows, type StorageRow } from "@/model/database-parts"
import { engineName } from "@/model/databases"
import { useSentence, useT, type Translate } from "@/i18n"
import { databaseLink, podEventsLink, serviceLink } from "@/lib/place"
import { useQuantityFormat } from "@/lib/quantity"
import { cn } from "@/lib/utils"
import {
  claimsOf,
  healthRank,
  hostUrl,
  isWarning,
  serviceId,
  statusOf,
  summarise,
  type Domain,
  type Event,
  type Project,
  type Service,
} from "@/model/service"
import { folded, serviceOf, type Alert } from "@/model/alerts"
import { activityTone, warningTone, warningVariant } from "@/model/severity"
import { refusedPlaces } from "@/routes/project/data"
import { FilterBox } from "@/routes/project/filter-box"
import { MeterStack, meterPartTone } from "@zeabur/design-system/components/meter"

/** Rows a side block lists before it says "Show all". */
const SIDE_ROWS = 5

function Heading({
  id,
  title,
  count,
  children,
  className,
}: {
  id: string
  title: ReactNode
  count?: ReactNode
  children?: ReactNode
  className?: string
}) {
  return (
    <div className={cn("flex flex-wrap items-center gap-x-2.5 gap-y-2", className)}>
      <h2 id={id} className="type-heading">
        {title}
      </h2>
      {count !== undefined ? (
        <span className="type-meta text-muted-foreground">{count}</span>
      ) : null}
      {children ? <div className="ml-auto flex items-center">{children}</div> : null}
    </div>
  )
}

const ACTIVITY_SHOWN = 12

type Entry = Schemas.TimelineEntry

/** Who wrote it: the tool by name, or the field manager when no tool is known. */
export const writerOf = (t: Translate, writer: Entry["writer"]) =>
  writer
    ? writer.tool === "other" || writer.tool === "controller"
      ? writer.manager
      : t(`timeline.tool.${writer.tool}`)
    : undefined

const isTrouble = (entry: Entry) => entry.kind === "warning" || entry.kind === "termination"

/** How many warnings, and how many entries in all, Activity lists. */
function useActivityCounts(project: string, events: Event[]) {
  const items = useQuery(projectTimelineQuery(project)).data?.items
  return {
    trouble: items ? items.filter(isTrouble).length : events.filter(isWarning).length,
    total: items ? items.length : events.length,
  }
}

/**
 * What happened in the project, newest first: releases, writes and who made them, warnings and
 * crashes. Before the timeline loads, or where it cannot be read, the events stand in. `framed`
 * holds the list in a frame of its own height; `bare` leaves out the heading, for a drawer.
 */
export function Activity({
  project,
  services,
  events,
  unreadable,
  framed = false,
  bare = false,
}: {
  project: string
  services: Service[]
  events: Event[]
  unreadable: Schemas.UnreadableView[]
  framed?: boolean
  bare?: boolean
}) {
  const t = useT()
  const timeline = useQuery(projectTimelineQuery(project))
  const [all, setAll] = useState(false)
  const items = timeline.data?.items
  const { trouble, total } = useActivityCounts(project, events)
  const limit = all || framed || bare ? undefined : ACTIVITY_SHOWN
  const control = (
    <Segmented
      label={t("events.show")}
      value={all ? "all" : "warnings"}
      onChange={(value) => setAll(value === "all")}
      options={[
        { value: "warnings", label: counted(t("events.warnings"), trouble) },
        { value: "all", label: counted(t("common.all"), total) },
      ]}
    />
  )
  const list = items ? (
    <TimelineEntries
      project={project}
      services={services}
      entries={(all ? items : items.filter(isTrouble)).slice(0, limit)}
      empty={all ? t("timeline.empty") : t("events.noWarnings")}
    />
  ) : (
    <EventEntries
      project={project}
      services={services}
      events={(all ? events : events.filter(isWarning)).slice(0, limit)}
      empty={all ? t("events.empty") : t("events.noWarnings")}
    />
  )
  return (
    <section
      aria-labelledby={bare ? undefined : "project-activity"}
      className="flex flex-col gap-3"
    >
      {bare ? (
        <div className="flex">{control}</div>
      ) : (
        <Heading id="project-activity" title={t("project.activity")}>
          {control}
        </Heading>
      )}
      <RefusedKinds unreadable={refusedAmong(unreadable, refusedPlaces.events)} line />
      {framed ? (
        <div className="max-h-120 overflow-y-auto rounded-xl border border-v2-border bg-card px-4 py-3">
          {list}
        </div>
      ) : (
        list
      )}
    </section>
  )
}

/** Whether a sentence already names the workload, so its line under it need not. */
const names = (entry: Entry) =>
  Object.values(entry.message.params ?? {}).some(
    (value) => typeof value === "string" && value.includes(entry.workload.name),
  )

/** Timeline entries as a list. */
function TimelineEntries({
  project,
  services,
  entries,
  empty,
}: {
  project: string
  services: Service[]
  entries: Entry[]
  empty: string
}) {
  const t = useT()
  const write = useSentence()
  if (entries.length === 0)
    return (
      <p className="flex items-center gap-2 text-sm text-muted-foreground">
        <StatusDot health="ok" />
        {empty}
      </p>
    )
  return (
    <Timeline>
      {entries.map((entry, i) => {
        const service = services.find(
          (s) => s.kind === entry.workload.kind && s.name === entry.workload.name,
        )
        const who = writerOf(t, entry.writer)
        const named = names(entry)
        const params = entry.message.params ?? {}
        const replica = service ? service.pods.find((p) => p.name === params.replica) : undefined
        const replicaName = params.replica ? (
          service && replica ? (
            <Link
              {...podEventsLink(project, service, replica.name)}
              className="hover:text-foreground hover:underline"
              translate="no"
            >
              {replica.name}
            </Link>
          ) : (
            <span translate="no">{params.replica}</span>
          )
        ) : null
        const tone = activityTone(entry, service && statusOf(service).health)
        return (
          <TimelineItem
            key={`${entry.at}/${entry.kind}/${entry.workload.name}/${i}`}
            tone={tone}
            time={<Ago iso={entry.at} />}
            title={
              <span className="wrap-anywhere">
                {write(entry.message)}
                {entry.count && entry.count > 1 && params.count === undefined ? (
                  <span className="text-muted-foreground"> ×{entry.count}</span>
                ) : null}
              </span>
            }
            meta={
              named && !who && !replicaName ? undefined : (
                <span className="flex flex-wrap items-center gap-x-1.5">
                  {named ? null : service ? (
                    <Link
                      {...serviceLink(project, service)}
                      className="hover:text-foreground hover:underline"
                      translate="no"
                    >
                      {entry.workload.name}
                    </Link>
                  ) : (
                    <span translate="no">{entry.workload.name}</span>
                  )}
                  {replicaName ? (
                    <span>
                      {t("events.forPod")} {replicaName}
                    </span>
                  ) : null}
                  {who ? <span>{t("timeline.by", who)}</span> : null}
                </span>
              )
            }
          >
            {entry.kind === "warning" && params.note ? (
              <p className="type-meta break-words text-muted-foreground" translate="no">
                {params.note}
              </p>
            ) : undefined}
          </TimelineItem>
        )
      })}
    </Timeline>
  )
}

function EventEntries({
  project,
  services,
  events,
  empty,
}: {
  project: string
  services: Service[]
  events: Event[]
  empty: string
}) {
  const t = useT()
  const serviceOf = (event: Event) => {
    const regarding = event.regarding
    if (!regarding) return undefined
    return services.find((s) =>
      regarding.kind === "Pod"
        ? s.pods.some((p) => p.name === regarding.name)
        : s.kind === regarding.kind && s.name === regarding.name,
    )
  }
  const objectLink = (event: Event, service: Service | undefined) => {
    const regarding = event.regarding
    if (!regarding) return null
    if (!service)
      return <span translate="no">{`${regarding.kind.toLowerCase()}/${regarding.name}`}</span>
    const link = (
      <Link
        {...serviceLink(project, service, { tab: "events" })}
        className="hover:text-foreground hover:underline"
        translate="no"
      >
        {service.name}
      </Link>
    )
    if (regarding.kind !== "Pod") return link
    return (
      <span className="flex flex-wrap items-center gap-x-1.5">
        {link}
        <span>
          {t("events.forPod")}{" "}
          <Link
            {...podEventsLink(project, service, regarding.name)}
            className="hover:text-foreground hover:underline"
            translate="no"
          >
            {regarding.name}
          </Link>
        </span>
      </span>
    )
  }
  if (events.length === 0)
    return (
      <p className="flex items-center gap-2 text-sm text-muted-foreground">
        <StatusDot health="ok" />
        {empty}
      </p>
    )
  return (
    <Timeline>
      {events.map((event) => {
        const service = serviceOf(event)
        return (
          <TimelineItem
            key={event.id}
            tone={
              isWarning(event)
                ? warningTone(warningVariant(event), service && statusOf(service).health)
                : "neutral"
            }
            time={event.lastSeen ? <Ago iso={event.lastSeen} /> : undefined}
            title={
              <span>
                <span className="font-medium" translate="no">
                  {event.reason}
                </span>
                {event.count > 1 ? (
                  <span className="text-muted-foreground"> ×{event.count}</span>
                ) : null}
              </span>
            }
            meta={objectLink(event, service)}
          >
            {event.note ? (
              <span className="line-clamp-2 text-muted-foreground">{event.note}</span>
            ) : undefined}
          </TimelineItem>
        )
      })}
    </Timeline>
  )
}

function counted(label: string, count: number) {
  return (
    <>
      {label}
      <span className="font-normal text-muted-foreground tabular-nums">{count}</span>
    </>
  )
}

const sideRow = "flex items-center gap-2.5 border-t border-rule py-2.75"

/** The rows a side block shows: all of a few, else the first `SIDE_ROWS`. */
const firstRows = <T,>(list: T[]) => (list.length > SIDE_ROWS + 1 ? list.slice(0, SIDE_ROWS) : list)

/** "Show all N" under a side block, opening every row in a drawer with a filter. */
function ShowAll({
  count,
  title,
  filterLabel,
  children,
}: {
  count: number
  title: string
  filterLabel: string
  children: (needle: string) => ReactNode
}) {
  const t = useT()
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState("")
  return (
    <>
      <Button
        variant="ghost"
        size="xs"
        className="-ml-2.5 self-start"
        onClick={() => setOpen(true)}
      >
        {t("services.showAll", count)}
      </Button>
      <Drawer
        open={open}
        onOpenChange={setOpen}
        title={title}
        closeLabel={t("common.close")}
        size="xl"
      >
        <div className="flex flex-col gap-3">
          <FilterBox value={query} onChange={setQuery} label={filterLabel} className="sm:w-64" />
          {children(query.trim().toLowerCase())}
        </div>
      </Drawer>
    </>
  )
}

const domainRank = (d: Domain) =>
  (d.https === false ? 0 : 10) + healthRank[statusOf(d.service).health]

/** Domains without HTTPS, then those of failing services, then the rest by host. */
const byException = (domains: Domain[]) =>
  domains.toSorted((a, b) => domainRank(a) - domainRank(b) || a.host.localeCompare(b.host))

function DomainRow({ project, domain }: { project: string; domain: Domain }) {
  const t = useT()
  const Lock =
    domain.https === undefined ? GlobeIcon : domain.https ? LockSimpleIcon : LockSimpleOpenIcon
  return (
    <li className={sideRow}>
      <Tooltip
        content={
          domain.https === undefined
            ? t("domains.gatewayHttps")
            : domain.https
              ? t("domains.https")
              : "HTTP"
        }
      >
        <Lock
          aria-hidden="true"
          className={cn("size-3.5 shrink-0", domain.https ? "text-ok" : "text-muted-foreground")}
        />
      </Tooltip>
      <div className="flex min-w-0 flex-1 flex-col">
        {domain.https ? <span className="sr-only">{t("domains.https")}</span> : null}
        <a
          href={hostUrl(domain)}
          target="_blank"
          rel="noreferrer noopener"
          className="truncate type-mono hover:underline"
          translate="no"
        >
          {domain.host}
          <span className="sr-only"> {t("common.newTab")}</span>
        </a>
        <span className="flex min-w-0 items-center gap-1.5 type-meta text-muted-foreground">
          <ArrowRightIcon className="size-3 shrink-0" aria-hidden="true" />
          <Link
            {...serviceLink(project, domain.service, { tab: "networking" })}
            className="truncate hover:text-foreground hover:underline"
            translate="no"
          >
            {domain.service.name}
          </Link>
          <StatusDot health={statusOf(domain.service).health} className="size-1.5" />
        </span>
      </div>
    </li>
  )
}

/** Every domain, filtered by host or service. */
function DomainList({
  project,
  domains,
  needle = "",
}: {
  project: string
  domains: Domain[]
  needle?: string
}) {
  const t = useT()
  const rows = domains.filter(
    (d) => !needle || `${d.host} ${d.service.name}`.toLowerCase().includes(needle),
  )
  if (rows.length === 0)
    return <p className="type-meta text-muted-foreground">{t("domains.noMatch")}</p>
  return (
    <ul className="flex flex-col">
      {rows.map((domain) => (
        <DomainRow key={domain.host} project={project} domain={domain} />
      ))}
    </ul>
  )
}

export function Domains({
  project,
  data,
  refused,
  bare = false,
}: {
  project: string
  data: Project
  refused: Schemas.UnreadableView[]
  /** Every domain with a filter, without the heading: in a drawer. */
  bare?: boolean
}) {
  const t = useT()
  const [query, setQuery] = useState("")
  const domains = byException(summarise(data).domains)
  if (domains.length === 0 && refused.length === 0) return null
  if (bare)
    return (
      <div className="flex flex-col gap-3">
        <RefusedKinds unreadable={refused} line />
        <FilterBox value={query} onChange={setQuery} label={t("domains.filter")} />
        <DomainList project={project} domains={domains} needle={query.trim().toLowerCase()} />
      </div>
    )
  const shown = firstRows(domains)
  const insecure = domains.filter((d) => d.https === false).length
  return (
    <section aria-labelledby="project-domains" className="flex flex-col gap-1">
      <Heading id="project-domains" title={t("common.domains")} count={domains.length} />
      <RefusedKinds unreadable={refused} line />
      {shown.length < domains.length && insecure > 0 ? (
        <p className="flex items-center gap-1.5 pb-1.5 type-meta text-muted-foreground">
          <LockSimpleOpenIcon className="size-3.5" aria-hidden="true" />
          {t("domains.withoutHttps", insecure)}
        </p>
      ) : null}
      {domains.length === 0 ? null : <DomainList project={project} domains={shown} />}
      {shown.length < domains.length ? (
        <ShowAll
          count={domains.length}
          title={t("common.domains")}
          filterLabel={t("domains.filter")}
        >
          {(needle) => <DomainList project={project} domains={domains} needle={needle} />}
        </ShowAll>
      ) : null}
    </section>
  )
}

/** The largest volumes get a colour each; the rest share the last, as "Other volumes". */
const METER_PARTS = 3

interface Sized {
  row: StorageRow
  bytes?: number
  /** Its part of the meter. */
  part: number
}

const unusedVolume = (v: Sized) =>
  !v.row.database && v.row.claims.every((claim) => claim.services.length === 0)

function useVolumes(project: string, data: Project) {
  const databases = useDatabases(project).data?.items ?? []
  // Claims from the whole project, so a database's volumes keep the size its workload mounts.
  const whole = useQuery(projectQuery(project)).data ?? data
  const rows = storageRows(claimsOf(whole), databases)
  const largest = rows
    .filter((r) => r.bytes)
    .toSorted((a, b) => (b.bytes ?? 0) - (a.bytes ?? 0))
    .map((r) => r.key)
  const sized: Sized[] = rows
    .map((row) => {
      const rank = largest.indexOf(row.key)
      return { row, bytes: row.bytes, part: rank < 0 ? -1 : Math.min(rank, METER_PARTS) }
    })
    // Unused first, then the largest.
    .toSorted(
      (a, b) =>
        Number(unusedVolume(b)) - Number(unusedVolume(a)) ||
        (b.bytes ?? 0) - (a.bytes ?? 0) ||
        a.row.key.localeCompare(b.row.key),
    )
  const parts = Array.from({ length: METER_PARTS + 1 }, (_, part) =>
    sized.filter((s) => s.part === part).reduce((sum, s) => sum + (s.bytes ?? 0), 0),
  )
  return {
    sized,
    parts,
    total: sized.reduce((sum, c) => sum + (c.bytes ?? 0), 0),
    known: sized.some((c) => c.bytes !== undefined),
    unused: sized.filter(unusedVolume).length,
    count: rows.reduce((sum, r) => sum + r.claims.length, 0),
  }
}

const volumeNames = (row: StorageRow) => row.claims.map((claim) => claim.name).join(", ")

function VolumeRow({ project, volume }: { project: string; volume: Sized }) {
  const t = useT()
  const { memoryParts, amount } = useQuantityFormat()
  const { row, bytes, part } = volume
  const [claim] = row.claims
  const services = row.claims.flatMap((c) => c.services)
  const [value, unit] = bytes !== undefined ? memoryParts(bytes) : [undefined, undefined]
  return (
    <li className={sideRow}>
      <span
        aria-hidden="true"
        className={cn(
          "size-2 shrink-0",
          part >= 0 ? meterPartTone(part) : "hairline-ring ring-idle ring-inset",
        )}
      />
      <div className="flex min-w-0 flex-1 flex-col">
        {row.database ? (
          <>
            <Link
              {...databaseLink(project, row.database)}
              className="truncate hover:underline"
              translate="no"
            >
              <span className="type-mono">{row.database.name}</span>{" "}
              <span className="text-muted-foreground">({engineName(row.database.engine)})</span>
            </Link>
            <span className="truncate type-meta type-mono text-muted-foreground" translate="no">
              {volumeNames(row)}
            </span>
          </>
        ) : (
          <>
            <span className="truncate type-mono" translate="no">
              {claim?.name}
            </span>
            {services.length > 0 ? (
              <span className="truncate type-meta text-muted-foreground">
                {services.map((service, j) => (
                  <span key={serviceId(service)}>
                    {j > 0 ? ", " : ""}
                    <Link
                      {...serviceLink(project, service, { tab: "storage" })}
                      className="hover:text-foreground hover:underline"
                      translate="no"
                    >
                      {service.name}
                    </Link>
                  </span>
                ))}
              </span>
            ) : null}
          </>
        )}
      </div>
      {unusedVolume(volume) ? (
        <Tooltip content={t("storage.notMounted")}>
          <span className="flex shrink-0">
            <StatusPill health="warn" size="sm" dot={false}>
              {t("resource.unused")}
            </StatusPill>
          </span>
        </Tooltip>
      ) : value ? (
        <span className="shrink-0">
          {value} <span className="type-meta text-muted-foreground">{unit}</span>
        </span>
      ) : (
        <span className="shrink-0 text-muted-foreground">
          {claim?.size ? amount(claim.size, "bytes") : "—"}
        </span>
      )}
    </li>
  )
}

function VolumeList({
  project,
  volumes,
  needle = "",
}: {
  project: string
  volumes: Sized[]
  needle?: string
}) {
  const t = useT()
  const rows = volumes.filter(
    (v) =>
      !needle ||
      [
        v.row.database?.name ?? "",
        ...v.row.claims.flatMap((c) => [c.name, ...c.services.map((s) => s.name)]),
      ]
        .join(" ")
        .toLowerCase()
        .includes(needle),
  )
  if (rows.length === 0)
    return <p className="type-meta text-muted-foreground">{t("storage.noMatch")}</p>
  return (
    <ul className="flex flex-col">
      {rows.map((volume) => (
        <VolumeRow key={volume.row.key} project={project} volume={volume} />
      ))}
    </ul>
  )
}

export function Storage({
  project,
  data,
  refused,
  bare = false,
}: {
  project: string
  data: Project
  refused: Schemas.UnreadableView[]
  /** Every volume with a filter, without the heading: in a drawer. */
  bare?: boolean
}) {
  const t = useT()
  const { memoryParts } = useQuantityFormat()
  const [query, setQuery] = useState("")
  const { sized, parts, total, known, unused } = useVolumes(project, data)
  const [totalValue, totalUnit] = memoryParts(total)
  if (sized.length === 0 && refused.length === 0) return null
  const size = `${totalValue} ${totalUnit}`
  const meter = total > 0 ? <MeterStack parts={parts} className="my-1.5" /> : null
  if (bare)
    return (
      <div className="flex flex-col gap-3">
        <RefusedKinds unreadable={refused} line />
        {known ? (
          <p className="type-meta text-muted-foreground">
            {t("storage.onVolumes", size, sized.length)}
          </p>
        ) : null}
        {meter}
        <FilterBox value={query} onChange={setQuery} label={t("storage.filter")} />
        <VolumeList project={project} volumes={sized} needle={query.trim().toLowerCase()} />
      </div>
    )
  const shown = firstRows(sized)
  const many = shown.length < sized.length
  const count = many
    ? [known ? t("storage.onVolumes", size, sized.length) : String(sized.length)]
        .concat(unused > 0 ? [t("resource.unusedCount", unused)] : [])
        .join(t("common.sep"))
    : known
      ? t("storage.requestedTotal", size)
      : undefined
  return (
    <section aria-labelledby="project-storage" className="flex flex-col gap-1">
      <Heading id="project-storage" title={t("tab.storage")} count={count} />
      <RefusedKinds unreadable={refused} line />
      {sized.length === 0 ? null : (
        <>
          {meter}
          <VolumeList project={project} volumes={shown} />
          {many ? (
            <ShowAll
              count={sized.length}
              title={t("tab.storage")}
              filterLabel={t("storage.filter")}
            >
              {(needle) => <VolumeList project={project} volumes={sized} needle={needle} />}
            </ShowAll>
          ) : null}
        </>
      )}
    </section>
  )
}

/** The project's object storage, when there is some or the operator is installed to make it. */
export function ObjectStorageSection({
  project,
  bare = false,
}: {
  project: string
  bare?: boolean
}) {
  const t = useT()
  const data = useObjectStorage(project).data
  if (!data) return null
  if (data.items.length === 0 && data.state !== "installed" && data.state !== "refused") return null
  return (
    <StoreSection
      bare={bare}
      id="project-object-storage"
      title={t("s3.title")}
      count={data.items.length}
      button={(size) => <NewObjectStorageButton project={project} data={data} size={size} />}
    >
      {data.items.length === 0 && data.state === "installed" ? (
        <p className="border-t border-rule py-2.5 type-meta text-muted-foreground">
          {t("s3.empty")}
        </p>
      ) : (
        <ul className="flex flex-col">
          {data.items.map((store) => (
            <ObjectStorageRow key={store.name} project={project} store={store} />
          ))}
        </ul>
      )}
      <ObjectStorageNotes data={data} />
    </StoreSection>
  )
}

/** The project's databases, when there are some or an operator is installed to make one. */
export function Databases({ project, bare = false }: { project: string; bare?: boolean }) {
  const t = useT()
  const data = useDatabases(project).data
  if (!data) return null
  if (data.items.length === 0 && installedEngines(data).length === 0) return null
  const body = (
    <>
      {data.items.length === 0 ? (
        <p className="border-t border-rule py-2.5 type-meta text-muted-foreground">
          {t("db.empty")}
        </p>
      ) : (
        <ul className="flex flex-col">
          {data.items.map((database) => (
            <DatabaseRow
              key={`${database.engine}/${database.name}`}
              project={project}
              database={database}
            />
          ))}
        </ul>
      )}
      <OperatorNotes data={data} />
    </>
  )
  return (
    <StoreSection
      bare={bare}
      id="project-databases"
      title={t("db.title")}
      count={data.items.length}
      button={(size) => <NewDatabaseButton project={project} data={data} size={size} />}
    >
      {body}
    </StoreSection>
  )
}

/** A side section of what operators run; `bare` in a drawer, which has the title. */
function StoreSection({
  bare,
  id,
  title,
  count,
  button,
  children,
}: {
  bare: boolean
  id: string
  title: string
  count: number
  button: (size: "xs" | "sm") => ReactNode
  children: ReactNode
}) {
  if (bare)
    return (
      <div className="flex flex-col gap-1">
        <div className="flex">{button("sm")}</div>
        {children}
      </div>
    )
  return (
    <section aria-labelledby={id} className="flex flex-col gap-1">
      <Heading id={id} title={title} count={count || undefined}>
        {button("xs")}
      </Heading>
      {children}
    </section>
  )
}

/** Backed-up volumes out of all of them, or undefined before the answer or without volumes. */
function useBackupCount(project: string) {
  const data = useBackups(project).data
  if (!data) return undefined
  const targets = data.coverage.filter((c) => c.target.kind !== "namespace")
  if (targets.length === 0) return undefined
  return {
    scheduled: targets.filter((c) => c.scheduledBy.length > 0).length,
    total: targets.length,
  }
}

export function Backups({ project, bare = false }: { project: string; bare?: boolean }) {
  const t = useT()
  const backups = useBackups(project)
  const data = backups.data
  if (!data) return null
  const targets = data.coverage.filter((c) => c.target.kind !== "namespace")
  if (targets.length === 0) return null
  const scheduled = targets.filter((c) => c.scheduledBy.length > 0).length
  const missing = targets.length - scheduled
  const rows = shownCoverage(data.coverage)
  const list = (
    <ul className="flex flex-col">
      {rows.map((c) => (
        <CoverageRow key={`${c.target.kind}/${c.target.name}`} coverage={c} />
      ))}
    </ul>
  )
  // Two or more volumes without a schedule are one sentence, not a row each saying so.
  const summarised = missing > 1 || rows.length > SIDE_ROWS + 1
  const body = (
    <>
      {missing > 1 ? (
        <p className="flex items-center gap-2 py-1.5 text-sm">
          <StatusDot health="warn" />
          {t("backups.unbacked", missing, targets.length)}
        </p>
      ) : null}
      {summarised && !bare ? (
        <Collapsible>
          <CollapsibleTrigger variant="disclosure">
            <CaretRightIcon aria-hidden="true" />
            {t("backups.volumes", rows.length)}
          </CollapsibleTrigger>
          <CollapsibleContent>{list}</CollapsibleContent>
        </Collapsible>
      ) : (
        list
      )}
      <ProviderNotes data={data} />
      <BackupDetails data={data} />
    </>
  )
  if (bare) return <div className="flex flex-col gap-1">{body}</div>
  return (
    <section aria-labelledby="project-backups" className="flex flex-col gap-1">
      <Heading
        id="project-backups"
        title={t("backups.title")}
        count={t("backups.scheduledOf", scheduled, targets.length)}
      />
      {body}
    </section>
  )
}

/** What Falco saw in the project's containers, when there is something to say. */
function useProjectAlerts(project: string) {
  const data = useQuery(projectAlertsQuery(project)).data
  const said = data?.state === "refused" || (data?.state === "installed" && data.items.length > 0)
  return said ? data : undefined
}

export function Alerts({
  project,
  services,
  bare = false,
}: {
  project: string
  services: Service[]
  /** Every alert with a filter, without the heading: in a drawer. */
  bare?: boolean
}) {
  const t = useT()
  const data = useProjectAlerts(project)
  if (!data) return null
  const place = (alert: Alert) => {
    const service = serviceOf(alert, services)
    if (!service) return null
    return (
      <Link
        {...serviceLink(project, service)}
        className="truncate font-medium text-foreground hover:underline"
        translate="no"
      >
        {service.name}
      </Link>
    )
  }
  const matching = (needle: string) =>
    data.items.filter(
      (a) =>
        !needle ||
        [a.rule, a.pod, a.container, a.command, serviceOf(a, services)?.name]
          .join(" ")
          .toLowerCase()
          .includes(needle),
    )
  const rows = folded(data.items).length
  if (bare)
    return (
      <div className="flex flex-col gap-3">
        <AlertsRefused data={data} />
        <RecentNote />
        <AlertList alerts={data.items} place={place} />
      </div>
    )
  return (
    <section aria-labelledby="project-alerts" className="flex flex-col gap-1">
      <Heading
        id="project-alerts"
        title={t("alerts.title")}
        count={data.state === "installed" ? rows : undefined}
      />
      <AlertsRefused data={data} />
      {data.items.length > 0 ? (
        <>
          <AlertList alerts={data.items} place={place} limit={SIDE_ROWS} />
          {rows > SIDE_ROWS ? (
            <ShowAll count={rows} title={t("alerts.title")} filterLabel={t("alerts.filter")}>
              {(needle) => (
                <>
                  <RecentNote />
                  <AlertList alerts={matching(needle)} place={place} />
                </>
              )}
            </ShowAll>
          ) : (
            <RecentNote />
          )}
        </>
      ) : null}
    </section>
  )
}

/**
 * On a phone with many services: Alerts, Activity, Domains, Storage and Backups as one row of buttons,
 * each opening its section in a drawer.
 */
export function SectionChips({
  project,
  data,
  events,
  unreadable,
}: {
  project: string
  data: Project
  events: Event[]
  unreadable: Schemas.UnreadableView[]
}) {
  const t = useT()
  const [open, setOpen] = useState<
    "alerts" | "activity" | "domains" | "storage" | "databases" | "objectStorage" | "backups"
  >()
  const { trouble } = useActivityCounts(project, events)
  const domains = summarise(data).domains.length
  const volumes = claimsOf(data).length
  const backups = useBackupCount(project)
  const alerts = useProjectAlerts(project)
  const databases = useDatabases(project).data?.items.length ?? 0
  const stores = useObjectStorage(project).data?.items.length ?? 0
  const refused = (resources: readonly string[]) => refusedAmong(unreadable, resources)
  const chip = (key: NonNullable<typeof open>, label: string, count: ReactNode) => (
    <Button key={key} variant="outline" size="sm" onClick={() => setOpen(key)}>
      {label}
      <span className="font-normal text-muted-foreground tabular-nums">{count}</span>
    </Button>
  )
  const titles = {
    alerts: t("alerts.title"),
    activity: t("project.activity"),
    domains: t("common.domains"),
    storage: t("tab.storage"),
    databases: t("db.title"),
    objectStorage: t("s3.title"),
    backups: t("backups.title"),
  }
  const bodies = {
    alerts: () => <Alerts project={project} services={data.services} bare />,
    activity: () => (
      <Activity
        project={project}
        services={data.services}
        events={events}
        unreadable={unreadable}
        bare
      />
    ),
    domains: () => (
      <Domains project={project} data={data} refused={refused(refusedPlaces.routes)} bare />
    ),
    storage: () => (
      <Storage project={project} data={data} refused={refused(refusedPlaces.claims)} bare />
    ),
    databases: () => <Databases project={project} bare />,
    objectStorage: () => <ObjectStorageSection project={project} bare />,
    backups: () => <Backups project={project} bare />,
  }
  return (
    <nav aria-label={t("project.sections")} className="flex flex-wrap gap-2">
      {alerts
        ? chip(
            "alerts",
            t("alerts.title"),
            alerts.state === "refused" ? "—" : folded(alerts.items).length,
          )
        : null}
      {chip("activity", t("project.activity"), t("events.warningsCount", trouble))}
      {domains > 0 ? chip("domains", t("common.domains"), domains) : null}
      {volumes > 0 ? chip("storage", t("tab.storage"), volumes) : null}
      {databases > 0 ? chip("databases", t("db.title"), databases) : null}
      {stores > 0 ? chip("objectStorage", t("s3.title"), stores) : null}
      {backups
        ? chip("backups", t("backups.title"), `${backups.scheduled}/${backups.total}`)
        : null}
      <Drawer
        open={open !== undefined}
        onOpenChange={(next) => {
          if (!next) setOpen(undefined)
        }}
        title={open ? titles[open] : undefined}
        closeLabel={t("common.close")}
      >
        {open ? bodies[open]() : null}
      </Drawer>
    </nav>
  )
}
