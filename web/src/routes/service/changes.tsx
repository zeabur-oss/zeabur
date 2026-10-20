import { useQuery } from "@tanstack/react-query"
import { useState } from "react"
import ArrowCounterClockwiseIcon from "~icons/ph/arrow-counter-clockwise"

import type { Schemas } from "@/api/client"
import { Ago } from "@/components/bits"
import { LoadingRows } from "@/components/page"
import { usePlanner } from "@/components/plan/planner"
import { Problem } from "@/components/problem"
import { StatusPill } from "@/components/status"
import { YamlDiff } from "@/components/yaml"
import { releasesQuery, serviceTimelineQuery } from "@/api/queries"
import { Badge } from "@zeabur/design-system/components/badge"
import { Button } from "@zeabur/design-system/components/button"
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
import { changesOf, type ChangeEntry } from "@/model/changes"
import { entryTone, writerOf } from "@/routes/project/side"
import { imageTag, isTask, type Event, type Service } from "@/model/service"

type Release = Schemas["WorkloadRelease"]

export function ChangesSection({
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
  // null: the newest older release.
  const [picked, setPicked] = useState<number | null>(null)

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
  const others = items.filter((r) => r !== current)
  const compared = others.find((r) => r.revision === picked) ?? others[0]
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

  return (
    <div className="flex flex-col gap-8">
      <section aria-labelledby="changes-title" className="flex flex-col gap-4">
        <SectionHeader
          id="changes-title"
          title={t("changes.title")}
          description={t("changes.description")}
        />
        {timeline.data?.truncated ? (
          <p className="type-meta text-muted-foreground">{t("timeline.truncated")}</p>
        ) : null}
        <Timeline>
          {entries.map((entry) => (
            <Entry
              key={entryKey(entry)}
              entry={entry}
              compared={entry.kind === "release" && entry.release === compared}
              managed={managed}
              onCompare={(release) => setPicked(release.revision)}
              onRollback={rollback}
              onEvents={onEvents}
            />
          ))}
        </Timeline>
      </section>
      {current && compared ? <Comparison from={compared} to={current} /> : null}
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

function Entry({
  entry,
  compared,
  managed,
  onCompare,
  onRollback,
  onEvents,
}: {
  entry: ChangeEntry
  compared: boolean
  managed?: string | null
  onCompare: (release: Release) => void
  onRollback: (release: Release) => void
  onEvents: () => void
}) {
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
    const trouble = entry.entry.kind === "warning" || entry.entry.kind === "termination"
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
          trouble ? (
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
  return (
    <TimelineItem
      tone={release.current ? "ok" : "neutral"}
      time={time}
      title={
        <span className="flex flex-wrap items-center gap-2">
          {t("changes.release", release.revision)}
          {release.current ? <Badge variant="ok">{t("releases.current")}</Badge> : null}
          {release.pods.total > 0 ? (
            <StatusPill
              health={release.pods.ready === release.pods.total ? "ok" : "warn"}
              size="sm"
            >
              {t("pod.readyCount", release.pods.ready, release.pods.total)}
            </StatusPill>
          ) : null}
        </span>
      }
      meta={
        [release.changeCause, who ? t("timeline.by", who) : undefined]
          .filter(Boolean)
          .join(t("common.sep")) || undefined
      }
      actions={
        release.current ? undefined : (
          <>
            <Button
              variant="ghost"
              size="xs"
              aria-pressed={compared}
              onClick={() => onCompare(release)}
            >
              {t("releases.compare")}
            </Button>
            <RollbackButton managed={managed} onClick={() => onRollback(release)} />
          </>
        )
      }
    >
      <span className="flex min-w-0 flex-wrap gap-x-3 gap-y-0.5">
        {release.images.map((image) => (
          <span key={image} title={image} className="truncate font-mono text-xs" translate="no">
            {image.replace(/[:@][^:/@]+$/, "")}
            <span className="rounded bg-muted px-1">:{imageTag(image)}</span>
          </span>
        ))}
      </span>
    </TimelineItem>
  )
}

function RollbackButton({ managed, onClick }: { managed?: string | null; onClick: () => void }) {
  const t = useT()
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
    button
  )
}

function Comparison({ from, to }: { from: Release; to: Release }) {
  const t = useT()
  const before = from.templateYaml
  const after = to.templateYaml
  return (
    <section className="flex flex-col gap-2" aria-labelledby="releases-diff">
      <SectionHeader
        id="releases-diff"
        size="sm"
        title={t("releases.diff", from.revision, to.revision)}
      />
      {before === after ? (
        <p className="text-sm text-muted-foreground">{t("releases.same")}</p>
      ) : (
        <YamlDiff before={before} after={after} />
      )}
    </section>
  )
}
