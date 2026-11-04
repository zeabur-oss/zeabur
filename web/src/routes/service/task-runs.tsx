import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { useState } from "react"
import PauseCircleIcon from "~icons/ph/pause-circle"
import PlayIcon from "~icons/ph/play"
import PlayCircleIcon from "~icons/ph/play-circle"

import { taskRunsQuery } from "@/api/queries"
import type { Schemas } from "@/api/client"
import { useLogsReadable } from "@/api/logs-access"
import { Ago } from "@/components/bits"
import { GateTip, useGate } from "@/components/gate"
import type { Gate } from "@/model/rights"
import { LoadingRows } from "@/components/page"
import { usePlanner } from "@/components/plan/planner"
import { Problem } from "@/components/problem"
import { Schedule } from "@/components/schedule"
import { StatusDot } from "@/components/status"
import { useT } from "@/i18n"
import { podLogsLink } from "@/lib/place"
import type { Service } from "@/model/service"
import { Badge } from "@zeabur-oss/design-system/components/badge"
import { Button } from "@zeabur-oss/design-system/components/button"
import { Container } from "@zeabur-oss/design-system/components/container"
import { Tooltip } from "@zeabur-oss/design-system/components/tooltip"

type Run = Schemas.TaskRun

/** The expression as `describeCron` reads a zone: `CRON_TZ=` in front. */
export const zoned = (cron: string, zone?: string | null) =>
  zone ? `CRON_TZ=${zone} ${cron}` : cron

const runHealth = (run: Run) =>
  run.status === "failed" ? "fail" : run.status === "running" ? "ok" : "idle"

export function TaskWhen({ schedule }: { schedule: Schemas.TaskSchedule }) {
  const t = useT()
  return (
    <span className="flex flex-wrap items-center gap-x-2">
      <Schedule cron={zoned(schedule.cron, schedule.timeZone)} />
      {schedule.suspended ? <Badge variant="secondary">{t("tasks.paused")}</Badge> : null}
    </span>
  )
}

const RUNS_SHOWN = 10

/** A CronJob's schedule, Run now, and its runs with their logs. */
export function TaskRunsCard({ project, service }: { project: string; service: Service }) {
  const t = useT()
  const gate = useGate(project, "changeServices")
  const planner = usePlanner(project, undefined, true)
  const runs = useQuery(taskRunsQuery(project, service.name))
  const going = (runs.data?.runs ?? []).filter((run) => run.status === "running")
  return (
    <Container
      title={
        <span className="flex min-w-0 items-center gap-3">
          <span className="shrink-0">{t("tasks.runs")}</span>
          {service.schedule ? <TaskWhen schedule={service.schedule} /> : null}
        </span>
      }
      titleAction={
        <span className="flex items-center gap-2">
          <PauseButton service={service} gate={gate} planner={planner} />
          <RunNowButton service={service} going={going} gate={gate} planner={planner} />
        </span>
      }
      variant="list"
    >
      {runs.isPending ? (
        <div className="px-4 py-3">
          <LoadingRows label={t("common.loading")} rows={2} />
        </div>
      ) : runs.isError ? (
        <div className="px-4 py-3">
          <Problem error={runs.error} />
        </div>
      ) : (
        <RunList project={project} service={service} runs={runs.data.runs} />
      )}
      {planner.dialog}
    </Container>
  )
}

type Planner = ReturnType<typeof usePlanner>

/** Stops or starts runs on schedule. */
function PauseButton({
  service,
  gate,
  planner,
}: {
  service: Service
  gate: Gate
  planner: Planner
}) {
  const t = useT()
  if (!service.schedule || service.provenance.tool) return null
  const suspended = !!service.schedule.suspended
  return (
    <GateTip gate={gate}>
      <Button
        variant="outline"
        size="sm"
        disabled={!gate.open}
        onClick={() =>
          planner.plan({
            title: t(suspended ? "tasks.resumeTitle" : "tasks.pauseTitle", service.name),
            changes: [{ op: "editTask", cronJob: service.name, paused: !suspended }],
            service: { kind: service.kind, name: service.name },
          })
        }
      >
        {suspended ? <PlayCircleIcon aria-hidden="true" /> : <PauseCircleIcon aria-hidden="true" />}
        {t(suspended ? "action.resume" : "action.pause")}
      </Button>
    </GateTip>
  )
}

/** Run now, as the task's overlap rule lets it: not while a run is going when it skips, and
 * saying which runs stop when it replaces. */
function RunNowButton({
  service,
  going,
  gate,
  planner,
}: {
  service: Service
  going: Run[]
  gate: Gate
  planner: Planner
}) {
  const t = useT()
  const concurrency = service.schedule?.concurrency
  const skipped = concurrency === "forbid" && going.length > 0
  const stops = concurrency === "replace" && going.length > 0
  const button = (
    <Button
      variant="outline"
      size="sm"
      disabled={!gate.open || skipped}
      onClick={() =>
        planner.plan({
          title: t("tasks.runTitle", service.name),
          changes: [{ op: "runTask", cronJob: service.name }],
          service: { kind: service.kind, name: service.name },
          notes: stops
            ? [
                t(
                  "tasks.replaceStops",
                  t(
                    "common.list",
                    going.map((run) => run.name),
                  ),
                ),
              ]
            : undefined,
        })
      }
    >
      <PlayIcon aria-hidden="true" />
      {t("tasks.runNow")}
    </Button>
  )
  if (!skipped) return <GateTip gate={gate}>{button}</GateTip>
  return (
    <Tooltip content={t("tasks.overlapSkips")}>
      <span className="inline-flex" tabIndex={0}>
        {button}
        <span className="sr-only">{t("tasks.overlapSkips")}</span>
      </span>
    </Tooltip>
  )
}

function RunList({ project, service, runs }: { project: string; service: Service; runs: Run[] }) {
  const t = useT()
  const logs = useLogsReadable(project)
  const [all, setAll] = useState(false)
  if (runs.length === 0)
    return <p className="px-4 py-3 type-body text-secondary">{t("tasks.noRuns")}</p>
  const shown = all ? runs : runs.slice(0, RUNS_SHOWN)
  return (
    <ul className="flex max-h-96 flex-col divide-y divide-main overflow-y-auto">
      {shown.map((run) => (
        <RunRow key={run.name} project={project} service={service} run={run} logs={logs} />
      ))}
      {runs.length > RUNS_SHOWN && !all ? (
        <li className="px-4 py-2">
          <Button variant="ghost" size="sm" onClick={() => setAll(true)}>
            {t("tasks.allRuns", runs.length)}
          </Button>
        </li>
      ) : null}
    </ul>
  )
}

function RunRow({
  project,
  service,
  run,
  logs,
}: {
  project: string
  service: Service
  run: Run
  logs: boolean
}) {
  const t = useT()
  const pod = run.pods.at(-1)
  return (
    <li className="flex min-w-0 items-center gap-2.5 px-4 py-2.5">
      <StatusDot health={runHealth(run)} />
      <span className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="flex min-w-0 items-center gap-2 type-body">
          <span className="font-medium">{t(`tasks.run.${run.status}`)}</span>
          {run.manual ? <Badge variant="secondary">{t("tasks.manual")}</Badge> : null}
        </span>
        <span className="truncate type-meta text-muted-foreground" translate="no">
          {run.name}
        </span>
      </span>
      <Ago iso={run.started} className="shrink-0 type-meta text-muted-foreground" />
      {pod && logs ? (
        <Link
          {...podLogsLink(project, service, pod)}
          className="shrink-0 type-meta text-secondary outline-none hover:text-main focus-visible:underline"
          aria-label={t("pod.logsOf", pod)}
        >
          {t("tab.logs")}
        </Link>
      ) : null}
    </li>
  )
}
