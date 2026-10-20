// The project's facts beside its services: domains, storage, backups and what happened lately.
import { Link } from "@tanstack/react-router"
import { useState, type ReactNode } from "react"
import ArrowRightIcon from "~icons/ph/arrow-right"
import LockSimpleIcon from "~icons/ph/lock-simple"
import LockSimpleOpenIcon from "~icons/ph/lock-simple-open"

import type { Schemas } from "@/api/client"
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
import { useT } from "@/i18n"
import { podEventsLink, serviceLink } from "@/lib/place"
import { useQuantityFormat } from "@/lib/quantity"
import { cn } from "@/lib/utils"
import type { ChangeEntry } from "@/model/changes"
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
const newest = (a: { at?: string }, b: { at?: string }) =>
  (b.at ? Date.parse(b.at) : 0) - (a.at ? Date.parse(a.at) : 0)

/** What happened in the project, newest first. `changes` takes the plans and outside edits the
 * API reports; until then the events fill it. */
export function Activity({
  project,
  services,
  events,
  unreadable,
  changes = [],
}: {
  project: string
  services: Service[]
  events: Event[]
  unreadable: Schemas["UnreadableView"][]
  changes?: ChangeEntry[]
}) {
  const t = useT()
  const [all, setAll] = useState(false)
  const warnings = events.filter(isWarning)
  const shown = (all ? events : warnings).map((event) => ({
    at: event.lastSeen ?? undefined,
    event,
  }))
  const planned = changes.filter((c) => c.kind === "plan" || c.kind === "external")
  const entries = [
    ...shown.map((e) => ({ ...e, change: undefined })),
    ...planned.map((change) => ({ at: change.at, event: undefined, change })),
  ]
    .toSorted(newest)
    .slice(0, all ? undefined : ACTIVITY_SHOWN)

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

  return (
    <section aria-labelledby="project-activity" className="flex flex-col gap-3">
      <Heading id="project-activity" title={t("project.activity")}>
        <Segmented
          label={t("events.show")}
          value={all ? "all" : "warnings"}
          onChange={(value) => setAll(value === "all")}
          options={[
            { value: "warnings", label: counted(t("events.warnings"), warnings.length) },
            { value: "all", label: counted(t("common.all"), events.length) },
          ]}
        />
      </Heading>
      <RefusedKinds unreadable={refusedAmong(unreadable, refusedPlaces.events)} line />
      {entries.length === 0 ? (
        <p className="flex items-center gap-2 text-sm text-muted-foreground">
          <StatusDot health="ok" />
          {all ? t("events.empty") : t("events.noWarnings")}
        </p>
      ) : (
        <Timeline>
          {entries.map(({ event, change }) =>
            event ? (
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
            ) : change && (change.kind === "plan" || change.kind === "external") ? (
              <TimelineItem
                key={`${change.kind}/${change.at}/${change.summary}`}
                tone="brand"
                time={<Ago iso={change.at} />}
                title={change.summary}
                meta={change.by}
              />
            ) : null,
          )}
        </Timeline>
      )}
    </section>
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
