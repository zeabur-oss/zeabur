// The project's facts beside its services: domains, storage, backups and what happened lately.
import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { useState, type ReactNode } from "react"
import ArrowRightIcon from "~icons/ph/arrow-right"
import LockSimpleIcon from "~icons/ph/lock-simple"
import LockSimpleOpenIcon from "~icons/ph/lock-simple-open"

import type { Schemas } from "@/api/client"
import { projectTimelineQuery } from "@/api/queries"
import { Ago, Segmented } from "@/components/bits"
import { refusedAmong, RefusedKinds } from "@/components/refused"
import { StatusDot, StatusPill } from "@/components/status"
import { Timeline, TimelineItem } from "@zeabur/design-system/components/timeline"
import { Tooltip } from "@zeabur/design-system/components/tooltip"
import {
  BackupDetails,
  CoverageRow,
  ProviderNotes,
  shownCoverage,
  useBackups,
} from "@/components/backups"
import { useSentence, useT, type Translate } from "@/i18n"
import { podEventsLink, serviceLink } from "@/lib/place"
import { useQuantityFormat } from "@/lib/quantity"
import { cn } from "@/lib/utils"
import {
  claimsOf,
  hostUrl,
  isWarning,
  serviceId,
  statusOf,
  summarise,
  type Event,
  type Project,
  type Service,
} from "@/model/service"
import { refusedPlaces } from "@/routes/project/data"
import { MeterStack, meterPartTone } from "@zeabur/design-system/components/meter"

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

type Entry = Schemas["TimelineEntry"]

export const entryTone = {
  release: "brand",
  write: "neutral",
  warning: "warn",
  termination: "fail",
} as const

/** Who wrote it: the tool by name, or the field manager when no tool is known. */
export const writerOf = (t: Translate, writer: Entry["writer"]) =>
  writer
    ? writer.tool === "other" || writer.tool === "controller"
      ? writer.manager
      : t(`timeline.tool.${writer.tool}`)
    : undefined

const isTrouble = (entry: Entry) => entry.kind === "warning" || entry.kind === "termination"

/** What happened in the project, newest first: releases, writes and who made them, warnings and
 * crashes. Before the timeline loads, or where it cannot be read, the events stand in. */
export function Activity({
  project,
  services,
  events,
  unreadable,
}: {
  project: string
  services: Service[]
  events: Event[]
  unreadable: Schemas["UnreadableView"][]
}) {
  const t = useT()
  const timeline = useQuery(projectTimelineQuery(project))
  const [all, setAll] = useState(false)
  const items = timeline.data?.items
  const trouble = items ? items.filter(isTrouble).length : events.filter(isWarning).length
  const total = items ? items.length : events.length
  return (
    <section aria-labelledby="project-activity" className="flex flex-col gap-3">
      <Heading id="project-activity" title={t("project.activity")}>
        <Segmented
          label={t("events.show")}
          value={all ? "all" : "warnings"}
          onChange={(value) => setAll(value === "all")}
          options={[
            { value: "warnings", label: counted(t("events.warnings"), trouble) },
            { value: "all", label: counted(t("common.all"), total) },
          ]}
        />
      </Heading>
      <RefusedKinds unreadable={refusedAmong(unreadable, refusedPlaces.events)} line />
      {items ? (
        <TimelineEntries
          project={project}
          services={services}
          entries={(all ? items : items.filter(isTrouble)).slice(
            0,
            all ? undefined : ACTIVITY_SHOWN,
          )}
          empty={all ? t("timeline.empty") : t("events.noWarnings")}
        />
      ) : (
        <EventEntries
          project={project}
          services={services}
          events={(all ? events : events.filter(isWarning)).slice(
            0,
            all ? undefined : ACTIVITY_SHOWN,
          )}
          empty={all ? t("events.empty") : t("events.noWarnings")}
        />
      )}
    </section>
  )
}

/** Timeline entries as a list; also the service's Changes. */
function TimelineEntries({
  project,
  services,
  entries,
  empty,
  actions,
}: {
  project: string
  services?: Service[]
  entries: Entry[]
  empty: string
  actions?: (entry: Entry) => ReactNode
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
        const service = services?.find(
          (s) => s.kind === entry.workload.kind && s.name === entry.workload.name,
        )
        const who = writerOf(t, entry.writer)
        return (
          <TimelineItem
            key={`${entry.at}/${entry.kind}/${entry.workload.name}/${i}`}
            tone={entryTone[entry.kind]}
            time={<Ago iso={entry.at} />}
            title={
              <span className="wrap-anywhere">
                {write(entry.message)}
                {entry.count && entry.count > 1 ? (
                  <span className="text-muted-foreground"> ×{entry.count}</span>
                ) : null}
              </span>
            }
            meta={
              services ? (
                <span className="flex flex-wrap items-center gap-x-1.5">
                  {service ? (
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
                  {who ? <span>{t("timeline.by", who)}</span> : null}
                </span>
              ) : who ? (
                t("timeline.by", who)
              ) : undefined
            }
            actions={actions?.(entry)}
          />
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
  const objectLink = (event: Event) => {
    const regarding = event.regarding
    if (!regarding) return null
    const label = `${regarding.kind.toLowerCase()}/${regarding.name}`
    const service = services.find((s) =>
      regarding.kind === "Pod"
        ? s.pods.some((p) => p.name === regarding.name)
        : s.kind === regarding.kind && s.name === regarding.name,
    )
    if (!service) return <span translate="no">{label}</span>
    const link =
      regarding.kind === "Pod"
        ? podEventsLink(project, service, regarding.name)
        : serviceLink(project, service, { tab: "events" })
    return (
      <Link {...link} className="hover:text-foreground hover:underline" translate="no">
        {label}
      </Link>
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
      {events.map((event) => (
        <TimelineItem
          key={event.id}
          tone={isWarning(event) ? "warn" : "neutral"}
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
          meta={objectLink(event)}
        >
          {event.note ? (
            <span className="line-clamp-2 text-muted-foreground">{event.note}</span>
          ) : undefined}
        </TimelineItem>
      ))}
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

export function Domains({
  project,
  data,
  refused,
}: {
  project: string
  data: Project
  refused: Schemas["UnreadableView"][]
}) {
  const t = useT()
  const domains = summarise(data).domains
  if (domains.length === 0 && refused.length === 0) return null
  return (
    <section aria-labelledby="project-domains" className="flex flex-col gap-1">
      <Heading id="project-domains" title={t("common.domains")} count={domains.length} />
      <RefusedKinds unreadable={refused} line />
      {domains.length === 0 ? null : (
        <ul className="flex flex-col">
          {domains.map((domain) => {
            const Lock = domain.https ? LockSimpleIcon : LockSimpleOpenIcon
            return (
              <li key={domain.host} className={sideRow}>
                <Tooltip content={domain.https ? t("domains.https") : "HTTP"}>
                  <Lock
                    aria-hidden="true"
                    className={cn(
                      "size-3.5 shrink-0",
                      domain.https ? "text-ok" : "text-muted-foreground",
                    )}
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
                  <span className="flex items-center gap-1.5 type-meta text-muted-foreground">
                    <ArrowRightIcon className="size-3" aria-hidden="true" />
                    <Link
                      {...serviceLink(project, domain.service, { tab: "networking" })}
                      className="hover:text-foreground hover:underline"
                      translate="no"
                    >
                      {domain.service.name}
                    </Link>
                    <StatusDot health={statusOf(domain.service).health} className="size-1.5" />
                  </span>
                </div>
              </li>
            )
          })}
        </ul>
      )}
    </section>
  )
}

export function Storage({
  project,
  data,
  refused,
}: {
  project: string
  data: Project
  refused: Schemas["UnreadableView"][]
}) {
  const t = useT()
  const { memoryParts } = useQuantityFormat()
  const claims = claimsOf(data)
  const sized = claims.map((claim) => ({ claim, bytes: claim.sizeBytes ?? undefined }))
  const total = sized.reduce((sum, c) => sum + (c.bytes ?? 0), 0)
  const anyKnown = sized.some((c) => c.bytes !== undefined)
  const [totalValue, totalUnit] = memoryParts(total)
  if (claims.length === 0 && refused.length === 0) return null
  return (
    <section aria-labelledby="project-storage" className="flex flex-col gap-1">
      <Heading
        id="project-storage"
        title={t("tab.storage")}
        count={anyKnown ? t("storage.requestedTotal", `${totalValue} ${totalUnit}`) : undefined}
      />
      <RefusedKinds unreadable={refused} line />
      {claims.length === 0 ? null : (
        <>
          {total > 0 ? (
            <div className="my-1.5">
              <MeterStack parts={sized.map((c) => c.bytes ?? 0)} />
            </div>
          ) : null}
          <ul className="flex flex-col">
            {sized.map(({ claim, bytes }, i) => {
              const [value, unit] =
                bytes !== undefined ? memoryParts(bytes) : [undefined, undefined]
              return (
                <li key={claim.name} className={sideRow}>
                  <span
                    aria-hidden="true"
                    className={cn(
                      "size-2 shrink-0",
                      bytes ? meterPartTone(i) : "hairline-ring ring-idle ring-inset",
                    )}
                  />
                  <div className="flex min-w-0 flex-1 flex-col">
                    <span className="truncate type-mono" translate="no">
                      {claim.name}
                    </span>
                    {claim.services.length > 0 ? (
                      <span className="truncate type-meta text-muted-foreground">
                        {claim.services.map((service, j) => (
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
                  </div>
                  {claim.services.length === 0 ? (
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
                    <span className="shrink-0 text-muted-foreground">{claim.size ?? "—"}</span>
                  )}
                </li>
              )
            })}
          </ul>
        </>
      )}
    </section>
  )
}

export function Backups({ project }: { project: string }) {
  const t = useT()
  const backups = useBackups(project)
  const data = backups.data
  if (!data) return null
  const targets = data.coverage.filter((c) => c.target.kind !== "namespace")
  if (targets.length === 0) return null
  const scheduled = targets.filter((c) => c.scheduledBy.length > 0).length
  return (
    <section aria-labelledby="project-backups" className="flex flex-col gap-1">
      <Heading
        id="project-backups"
        title={t("backups.title")}
        count={t("backups.scheduledOf", scheduled, targets.length)}
      />
      <ul className="flex flex-col">
        {shownCoverage(data.coverage).map((c) => (
          <CoverageRow key={`${c.target.kind}/${c.target.name}`} coverage={c} />
        ))}
      </ul>
      <ProviderNotes data={data} />
      <BackupDetails data={data} />
    </section>
  )
}
