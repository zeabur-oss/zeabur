import { useQuery } from "@tanstack/react-query"
import { useState, type ReactNode } from "react"
import ArrowCounterClockwiseIcon from "~icons/ph/arrow-counter-clockwise"
import CaretRightIcon from "~icons/ph/caret-right"

import type { Schemas } from "@/api/client"
import { Ago } from "@/components/bits"
import { FilterField } from "@/components/filter-field"
import { LoadingRows } from "@/components/page"
import { GateTip, useHereGate } from "@/components/gate"
import { usePlanner } from "@/components/plan/planner"
import { Problem } from "@/components/problem"
import { StatusPill } from "@/components/status"
import { YamlDiff } from "@/components/yaml"
import { releasesQuery, serviceTimelineQuery } from "@/api/queries"
import { Badge } from "@zeabur/design-system/components/badge"
import { Button } from "@zeabur/design-system/components/button"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur/design-system/components/collapsible"
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@zeabur/design-system/components/empty"
import { Illustration } from "@zeabur/design-system/components/illustration"
import { SectionHeader } from "@zeabur/design-system/components/section-header"
import { Timeline, TimelineItem } from "@zeabur/design-system/components/timeline"
import { Tooltip } from "@zeabur/design-system/components/tooltip"
import { useSentence, useT } from "@/i18n"
import { cn } from "@/lib/utils"
import {
  changesOf,
  changeText,
  foldChanges,
  isTrouble,
  weekOf,
  type ChangeEntry,
} from "@/model/changes"
import { entryTone, writerOf } from "@/routes/project/side"
import { imageTag, isTask, type Event, type Service } from "@/model/service"

type Release = Schemas["WorkloadRelease"]

/** Row actions show on hover or focus where there is a pointer that hovers, always on touch. */
const quiet = "pointer-fine:opacity-0 pointer-fine:focus-within:opacity-100"

export function DeploymentsTab({
  project,
  service,
  events,
  onEvents,
}: {
  project: string
  service: Service
  events: Event[]
  onEvents: () => void
}) {
  const t = useT()
  const releases = useQuery(releasesQuery(project, service.kind, service.name))
  const timeline = useQuery(serviceTimelineQuery(project, service.kind, service.name))
  const planner = usePlanner(project)
  const [picked, setPicked] = useState<number>()
  const [now] = useState(() => Date.now())

  if (releases.isPending || timeline.isPending)
    return <LoadingRows label={t("common.loading")} rows={4} />
  if (releases.isError) return <Problem error={releases.error} />
  const items = releases.data.items
  const entries = changesOf(items, events, timeline.data?.items)
  if (entries.length === 0)
    return (
      <Empty size="sm">
        <EmptyHeader>
          <EmptyMedia>
            <Illustration kind="empty" seed="releases" />
          </EmptyMedia>
          <EmptyTitle>{t("changes.none")}</EmptyTitle>
          <EmptyDescription>
            {t(isTask(service.kind) ? "releases.noneTask" : "releases.none")}
          </EmptyDescription>
        </EmptyHeader>
      </Empty>
    )

  const current = items.find((r) => r.current) ?? items[0]
  const managed = service.provenance.tool
  const rollback = (release: Release) =>
    planner.plan({
      title: t("title.rollback", service.name, release.revision),
      changes: [
        {
          op: "rollback",
          workload: { kind: service.kind, name: service.name },
          revision: release.revision,
        },
      ],
    })
  const { recent, older } = foldChanges(entries, now)
  const week = weekOf(entries, now)
  const compare = (release: Release) =>
    setPicked((was) => (was === release.revision ? undefined : release.revision))
  const diff = (release: Release) =>
    current && picked === release.revision && release !== current ? (
      <Comparison from={release} to={current} />
    ) : null

  return (
    <div className="flex flex-col gap-8">
      <section aria-labelledby="changes-title" className="flex flex-col gap-4">
        <SectionHeader
          id="changes-title"
          title={t("changes.title")}
          description={
            current || older.length > 0 ? (
              <>
                {current ? (
                  <>
                    {t("changes.lead.current", current.revision)}
                    {current.created ? (
                      <>
                        {t("common.sep")}
                        {t("changes.lead.deployed")} <Ago iso={current.created} />
                      </>
                    ) : null}
                    {current.pods.total > 0 ? (
                      <>
                        {t("common.sep")}
                        {t("pod.readyCount", current.pods.ready, current.pods.total)}
                      </>
                    ) : null}
                    {t("changes.lead.stop")}
                  </>
                ) : null}
                {older.length > 0 ? t("changes.lead.week", week.releases, week.warnings) : null}
              </>
            ) : undefined
          }
        />
        {timeline.data?.truncated ? (
          <p className="type-meta text-muted-foreground">{t("timeline.truncated")}</p>
        ) : null}
        <Timeline aria-labelledby="changes-title">
          {recent.map((entry) => (
            <Entry
              key={entryKey(entry)}
              entry={entry}
              picked={picked}
              managed={managed}
              onCompare={compare}
              onRollback={rollback}
              onEvents={onEvents}
              diff={diff}
            />
          ))}
        </Timeline>
        {older.length > 0 ? (
          <Older
            entries={older}
            picked={picked}
            managed={managed}
            onCompare={compare}
            onRollback={rollback}
            onEvents={onEvents}
            diff={diff}
          />
        ) : null}
      </section>
      {planner.dialog}
    </div>
  )
}

const entryKey = (entry: ChangeEntry) =>
  entry.kind === "release"
    ? `release/${entry.release.name}`
    : entry.kind === "warning"
      ? `warning/${entry.event.id}`
      : `${entry.entry.kind}/${entry.at}/${entry.entry.message.key}`

interface RowProps {
  picked?: number
  managed?: string | null
  onCompare: (release: Release) => void
  onRollback: (release: Release) => void
  onEvents: () => void
  diff: (release: Release) => ReactNode
}

function Entry({
  entry,
  picked,
  managed,
  onCompare,
  onRollback,
  onEvents,
  diff,
}: RowProps & { entry: ChangeEntry }) {
  const t = useT()
  const write = useSentence()
  const time = entry.at ? <Ago iso={entry.at} /> : undefined
  if (entry.kind === "warning")
    return (
      <TimelineItem
        tone="warn"
        time={time}
        title={
          <span translate="no">
            {entry.event.reason}
            {entry.event.count > 1 ? (
              <span className="ml-1.5 font-normal text-tertiary">×{entry.event.count}</span>
            ) : null}
          </span>
        }
        actions={
          <Button variant="ghost" size="xs" onClick={onEvents}>
            {t("tab.events")}
          </Button>
        }
      >
        <span className="line-clamp-2" lang="en">
          {entry.event.note}
        </span>
      </TimelineItem>
    )
  if (entry.kind === "timeline") {
    const who = writerOf(t, entry.entry.writer)
    return (
      <TimelineItem
        tone={entryTone[entry.entry.kind]}
        time={time}
        title={
          <span>
            {write(entry.entry.message)}
            {entry.entry.count && entry.entry.count > 1 ? (
              <span className="ml-1.5 font-normal text-tertiary">×{entry.entry.count}</span>
            ) : null}
          </span>
        }
        meta={who ? t("timeline.by", who) : undefined}
        actions={
          isTrouble(entry) ? (
            <Button variant="ghost" size="xs" onClick={onEvents}>
              {t("tab.events")}
            </Button>
          ) : undefined
        }
      />
    )
  }
  const who = writerOf(t, entry.entry?.writer)
  const release = entry.release
  const selected = picked === release.revision
  return (
    <TimelineItem
      tone={release.current ? "ok" : "neutral"}
      time={time}
      title={<ReleaseTitle release={release} />}
      meta={
        [release.changeCause, who ? t("timeline.by", who) : undefined]
          .filter(Boolean)
          .join(t("common.sep")) || undefined
      }
      actions={
        release.current ? undefined : (
          <span
            className={cn(
              "flex items-center gap-1.5",
              !selected && quiet,
              "pointer-fine:group-hover/timeline:opacity-100",
            )}
          >
            <ReleaseActions
              release={release}
              selected={selected}
              managed={managed}
              onCompare={onCompare}
              onRollback={onRollback}
            />
          </span>
        )
      }
    >
      <Images release={release} />
      {diff(release)}
    </TimelineItem>
  )
}

function ReleaseTitle({ release }: { release: Release }) {
  const t = useT()
  return (
    <span className="flex flex-wrap items-center gap-2">
      {t("changes.release", release.revision)}
      {release.current ? <Badge variant="ok">{t("releases.current")}</Badge> : null}
      {release.pods.total > 0 ? (
        <StatusPill health={release.pods.ready === release.pods.total ? "ok" : "warn"} size="sm">
          {t("pod.readyCount", release.pods.ready, release.pods.total)}
        </StatusPill>
      ) : null}
    </span>
  )
}

function Images({ release }: { release: Release }) {
  return (
    <span className="flex min-w-0 flex-wrap gap-x-3 gap-y-0.5">
      {release.images.map((image) => (
        <span key={image} title={image} className="truncate font-mono text-xs" translate="no">
          {image.replace(/[:@][^:/@]+$/, "")}
          <span className="rounded bg-muted px-1">:{imageTag(image)}</span>
        </span>
      ))}
    </span>
  )
}

function ReleaseActions({
  release,
  selected,
  managed,
  onCompare,
  onRollback,
}: {
  release: Release
  selected: boolean
  managed?: string | null
  onCompare: (release: Release) => void
  onRollback: (release: Release) => void
}) {
  const t = useT()
  return (
    <>
      <Button variant="ghost" size="xs" aria-pressed={selected} onClick={() => onCompare(release)}>
        {t("releases.compare")}
      </Button>
      <RollbackButton managed={managed} onClick={() => onRollback(release)} />
    </>
  )
}

/** Everything past the timeline: a bounded table to filter, each release with its actions. */
function Older({ entries, ...row }: RowProps & { entries: ChangeEntry[] }) {
  const t = useT()
  const write = useSentence()
  const [query, setQuery] = useState("")
  const needle = query.trim().toLowerCase()
  const shown = needle
    ? entries.filter((e) =>
        changeText(e, (entry) => write(entry.message))
          .toLowerCase()
          .includes(needle),
      )
    : entries
  return (
    <Collapsible>
      <CollapsibleTrigger variant="disclosure">
        <CaretRightIcon aria-hidden="true" />
        {t("changes.older")}
        <span className="tabular-nums">{entries.length}</span>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="flex flex-col gap-2 pt-3">
          <FilterField value={query} onChange={setQuery} label={t("changes.filter")} />
          <div className="max-h-90 overflow-y-auto rounded-lg border border-v2-border bg-card">
            <table className="w-full text-sm" aria-label={t("changes.older")}>
              <thead className="sticky top-0 z-raised border-b border-v2-border bg-card">
                <tr>
                  <th className="w-28 px-3 py-1.5 text-left type-meta font-medium text-muted-foreground max-sm:w-20">
                    {t("changes.when")}
                  </th>
                  <th className="px-3 py-1.5 text-left type-meta font-medium text-muted-foreground">
                    {t("changes.what")}
                  </th>
                  <th className="px-3 py-1.5">
                    <span className="sr-only">{t("table.actions")}</span>
                  </th>
                </tr>
              </thead>
              <tbody className="divide-y divide-v2-border">
                {shown.map((entry) => (
                  <OlderRow key={entryKey(entry)} entry={entry} {...row} />
                ))}
                {shown.length === 0 ? (
                  <tr>
                    <td colSpan={3} className="px-3 py-2 text-xs text-muted-foreground">
                      {t("changes.noMatch")}
                    </td>
                  </tr>
                ) : null}
              </tbody>
            </table>
          </div>
        </div>
      </CollapsibleContent>
    </Collapsible>
  )
}

function OlderRow({
  entry,
  picked,
  managed,
  onCompare,
  onRollback,
  onEvents,
  diff,
}: RowProps & { entry: ChangeEntry }) {
  const t = useT()
  const write = useSentence()
  const release = entry.kind === "release" ? entry.release : undefined
  const selected = release !== undefined && picked === release.revision
  const what =
    entry.kind === "release" ? (
      <span className="flex min-w-0 flex-col gap-0.5">
        <span className="font-medium">{t("changes.release", entry.release.revision)}</span>
        <Images release={entry.release} />
      </span>
    ) : entry.kind === "warning" ? (
      <span className="flex min-w-0 items-baseline gap-2">
        <span className="font-medium text-warn-strong" translate="no">
          {entry.event.reason}
        </span>
        <span className="truncate text-xs text-muted-foreground" lang="en">
          {entry.event.note}
        </span>
      </span>
    ) : (
      <span className={cn(isTrouble(entry) && "text-warn-strong")}>
        {write(entry.entry.message)}
      </span>
    )
  return (
    <>
      <tr
        className="group/row hover:bg-table-row-hover"
        data-state={selected ? "selected" : undefined}
      >
        <td className="px-3 py-2 align-top text-xs whitespace-nowrap text-tertiary">
          {entry.at ? <Ago iso={entry.at} /> : null}
        </td>
        <td className="w-full max-w-0 px-3 py-2 align-top">{what}</td>
        <td className="px-3 py-1.5 text-right align-top whitespace-nowrap">
          {release ? (
            <span
              className={cn(
                "flex items-center justify-end gap-1.5",
                !selected && quiet,
                "pointer-fine:group-hover/row:opacity-100",
              )}
            >
              <ReleaseActions
                release={release}
                selected={selected}
                managed={managed}
                onCompare={onCompare}
                onRollback={onRollback}
              />
            </span>
          ) : isTrouble(entry) ? (
            <Button variant="ghost" size="xs" onClick={onEvents}>
              {t("tab.events")}
            </Button>
          ) : null}
        </td>
      </tr>
      {release && selected ? (
        <tr>
          <td colSpan={3} className="max-w-0 px-3 pb-3">
            {diff(release)}
          </td>
        </tr>
      ) : null}
    </>
  )
}

function RollbackButton({ managed, onClick }: { managed?: string | null; onClick: () => void }) {
  const t = useT()
  const gate = useHereGate("changeServices")
  const button = (
    <Button variant="outline" size="xs" disabled={!!managed} onClick={onClick}>
      <ArrowCounterClockwiseIcon aria-hidden="true" data-icon="inline-start" />
      {t("releases.rollback")}
    </Button>
  )
  return managed ? (
    <Tooltip content={t("common.managedByDetail", managed)}>
      <span>{button}</span>
    </Tooltip>
  ) : (
    <GateTip gate={gate}>{button}</GateTip>
  )
}

function Comparison({ from, to }: { from: Release; to: Release }) {
  const t = useT()
  const before = from.templateYaml
  const after = to.templateYaml
  return (
    <div className="mt-2 flex flex-col gap-1.5">
      <span className="type-meta font-medium text-secondary">
        {t("releases.diff", from.revision, to.revision)}
      </span>
      {before === after ? (
        <p className="text-sm text-muted-foreground">{t("releases.same")}</p>
      ) : (
        <YamlDiff before={before} after={after} />
      )}
    </div>
  )
}
