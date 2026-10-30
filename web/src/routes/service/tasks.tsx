import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { useState } from "react"
import PauseCircleIcon from "~icons/ph/pause-circle"
import PlayIcon from "~icons/ph/play"
import PlayCircleIcon from "~icons/ph/play-circle"
import PlusIcon from "~icons/ph/plus"

import { projectQuery, taskRunsQuery } from "@/api/queries"
import type { Schemas } from "@/api/client"
import { useLogsReadable } from "@/api/logs-access"
import { Ago } from "@/components/bits"
import { FormField } from "@/components/form-field"
import { GateNote, GateTip, useGate } from "@/components/gate"
import type { Gate } from "@/model/rights"
import { LoadingRows } from "@/components/page"
import { useHold } from "@/components/plan/draft"
import { PlanDrawer, ReviewFooter, ReviewForm } from "@/components/plan/plan-drawer"
import { usePlanner, usePlanStep } from "@/components/plan/planner"
import { Problem } from "@/components/problem"
import { Schedule } from "@/components/schedule"
import { StatusDot } from "@/components/status"
import { useT, type Translate } from "@/i18n"
import { describeCron } from "@/lib/cron"
import { podLogsLink, serviceLink } from "@/lib/place"
import type { Service } from "@/model/service"
import { ContainerField } from "@/routes/service/container-field"
import { Badge } from "@zeabur/design-system/components/badge"
import { Button } from "@zeabur/design-system/components/button"
import { Card, CardRow } from "@zeabur/design-system/components/card"
import { Container } from "@zeabur/design-system/components/container"
import { Input } from "@zeabur/design-system/components/input"
import { SectionHeader } from "@zeabur/design-system/components/section-header"
import { Select } from "@zeabur/design-system/components/select"
import { Tooltip } from "@zeabur/design-system/components/tooltip"

type Concurrency = Schemas.TaskConcurrency
type Run = Schemas.TaskRun

const NAME = /^[a-z0-9]([-a-z0-9]*[a-z0-9])?$/
const NAME_MAX = 52

const keepable = (value: string) => /^\d+$/.test(value) && Number(value) <= 100

/** The expression as `describeCron` reads a zone: `CRON_TZ=` in front. */
const zoned = (cron: string, zone?: string | null) => (zone ? `CRON_TZ=${zone} ${cron}` : cron)

const runHealth = (run: Run) =>
  run.status === "failed" ? "fail" : run.status === "running" ? "ok" : "idle"

/** The tasks made from a service, and the way to add one. */
export function TasksSection({ project, service }: { project: string; service: Service }) {
  const t = useT()
  const gate = useGate(project, "changeServices")
  const [adding, setAdding] = useState(false)
  const tasks = (useQuery(projectQuery(project)).data?.services ?? []).filter(
    (s) => s.kind === "CronJob" && s.schedule?.taskOf === service.name,
  )
  return (
    <section aria-labelledby="tasks-title" className="flex flex-col gap-3">
      <SectionHeader id="tasks-title" title={t("part.tasks")} description={t("tasks.hint")} />
      <GateNote gate={gate} />
      <Card variant="list">
        {tasks.map((task) => (
          <CardRow
            key={task.name}
            label={
              <Link
                {...serviceLink(project, task)}
                className="type-mono outline-none hover:underline focus-visible:underline"
                translate="no"
              >
                {task.name}
              </Link>
            }
            description={task.schedule ? <TaskWhen schedule={task.schedule} /> : undefined}
          />
        ))}
        <CardRow
          label={tasks.length === 0 ? t("tasks.none") : t("tasks.another")}
          description={tasks.length === 0 ? t("tasks.noneHint") : undefined}
        >
          <GateTip gate={gate}>
            <Button
              variant="outline"
              size="sm"
              disabled={!gate.open}
              onClick={() => setAdding(true)}
            >
              <PlusIcon aria-hidden="true" />
              {t("tasks.add")}
            </Button>
          </GateTip>
        </CardRow>
      </Card>
      {adding ? (
        <ScheduleTaskDrawer project={project} service={service} onClose={() => setAdding(false)} />
      ) : null}
    </section>
  )
}

function TaskWhen({ schedule }: { schedule: Schemas.TaskSchedule }) {
  const t = useT()
  return (
    <span className="flex flex-wrap items-center gap-x-2">
      <Schedule cron={zoned(schedule.cron, schedule.timeZone)} />
      {schedule.suspended ? <Badge variant="secondary">{t("tasks.paused")}</Badge> : null}
    </span>
  )
}

function taskProblems(
  t: Translate,
  form: {
    name: string
    command: string
    schedule: string
    keepSucceeded: string
    keepFailed: string
  },
) {
  const nameFits = NAME.test(form.name) && form.name.length <= NAME_MAX
  return {
    name: nameFits ? undefined : t("tasks.badName", NAME_MAX),
    command: form.command.trim() ? undefined : t("tasks.needCommand"),
    schedule: form.schedule.trim() ? undefined : t("tasks.needSchedule"),
    keepSucceeded: keepable(form.keepSucceeded) ? undefined : t("tasks.badKeep"),
    keepFailed: keepable(form.keepFailed) ? undefined : t("tasks.badKeep"),
  }
}

function ScheduleTaskDrawer({
  project,
  service,
  onClose,
}: {
  project: string
  service: Service
  onClose: () => void
}) {
  const t = useT()
  const step = usePlanStep(project, onClose)
  const containers = service.containers.filter((c) => !c.init)
  const [container, setContainer] = useState(containers[0]?.name ?? "")
  const [name, setName] = useState(`${service.name}-task`.slice(0, NAME_MAX))
  const [command, setCommand] = useState("")
  const [schedule, setSchedule] = useState("0 3 * * *")
  const [zone, setZone] = useState(() => Intl.DateTimeFormat().resolvedOptions().timeZone)
  const [concurrency, setConcurrency] = useState<Concurrency>("forbid")
  const [keepSucceeded, setKeepSucceeded] = useState("3")
  const [keepFailed, setKeepFailed] = useState("1")
  const [tried, setTried] = useState(false)
  const words = describeCron(t, zoned(schedule.trim(), zone.trim()))
  const problems = taskProblems(t, { name, command, schedule, keepSucceeded, keepFailed })
  const review = () => {
    setTried(true)
    if (Object.values(problems).some(Boolean)) return
    step.plan({
      title: t("tasks.planTitle", name, service.name),
      changes: [
        {
          op: "scheduleTask",
          from: { kind: service.kind, name: service.name },
          container: containers.length > 1 ? container : undefined,
          name,
          schedule: schedule.trim(),
          timeZone: zone.trim() || undefined,
          command: command.trim(),
          concurrency,
          keepSucceeded: Number(keepSucceeded),
          keepFailed: Number(keepFailed),
        },
      ],
    })
  }
  const shown = (message?: string) => (tried && message ? [message] : [])
  return (
    <PlanDrawer
      step={step}
      onClose={onClose}
      title={t("tasks.addTitle", service.name)}
      description={t("tasks.addHint", service.name)}
      footer={<ReviewFooter onCancel={onClose} onReview={review} />}
    >
      <ReviewForm onReview={review}>
        <FormField
          label={t("tasks.command")}
          hint={t("tasks.commandHint")}
          errors={shown(problems.command)}
        >
          <Input
            size="sm"
            mono
            value={command}
            placeholder="npm run cleanup"
            autoComplete="off"
            onChange={(event) => setCommand(event.target.value)}
          />
        </FormField>
        <FormField
          label={t("tasks.schedule")}
          hint={words ?? t("tasks.scheduleHint")}
          errors={shown(problems.schedule)}
        >
          <Input
            size="sm"
            mono
            value={schedule}
            autoComplete="off"
            onChange={(event) => setSchedule(event.target.value)}
          />
        </FormField>
        <FormField label={t("tasks.zone")} hint={t("tasks.zoneHint")}>
          <Input
            size="sm"
            mono
            value={zone}
            placeholder="Etc/UTC"
            autoComplete="off"
            onChange={(event) => setZone(event.target.value)}
          />
        </FormField>
        <FormField label={t("tasks.name")} errors={shown(problems.name)}>
          <Input
            size="sm"
            mono
            value={name}
            autoComplete="off"
            onChange={(event) => setName(event.target.value.trim())}
          />
        </FormField>
        <ContainerField containers={containers} value={container} onChange={setContainer} />
        <RunFields
          values={{ concurrency, keepSucceeded, keepFailed }}
          problems={tried ? problems : {}}
          onChange={(key, value) => {
            if (key === "concurrency") setConcurrency(value as Concurrency)
            else if (key === "keepSucceeded") setKeepSucceeded(value)
            else setKeepFailed(value)
          }}
        />
      </ReviewForm>
    </PlanDrawer>
  )
}

/** A CronJob's schedule and how it runs; edits wait in the draft. */
export function ScheduleSection({ project, service }: { project: string; service: Service }) {
  const t = useT()
  const gate = useGate(project, "changeServices")
  const current = service.schedule
  if (!current) return null
  const disabled = !!service.provenance.tool || !gate.open
  return (
    <section aria-labelledby="schedule-title" className="flex flex-col gap-3">
      <SectionHeader
        id="schedule-title"
        title={t("part.schedule")}
        description={t("tasks.settingsHint")}
      />
      {service.provenance.tool ? null : <GateNote gate={gate} />}
      <ScheduleForm cronJob={service.name} current={current} disabled={disabled} />
    </section>
  )
}

interface ScheduleValues {
  schedule: string
  zone: string
  command: string
  concurrency: Concurrency
  keepSucceeded: string
  keepFailed: string
}

const valuesOf = (current: Schemas.TaskSchedule): ScheduleValues => ({
  schedule: current.cron,
  zone: current.timeZone ?? "",
  command: current.command ?? "",
  concurrency: current.concurrency,
  keepSucceeded: String(current.keepSucceeded),
  keepFailed: String(current.keepFailed),
})

/** The fields that differ from the CronJob, as one edit; `undefined` when none do. */
function scheduleEdit(
  cronJob: string,
  current: Schemas.TaskSchedule,
  values: ScheduleValues,
): Extract<Schemas.Change, { op: "editTask" }> | undefined {
  const before = valuesOf(current)
  const trimmed = (key: keyof ScheduleValues) => values[key].trim()
  const changed = (key: keyof ScheduleValues) => trimmed(key) !== before[key]
  const edit = {
    op: "editTask" as const,
    cronJob,
    ...(changed("schedule") ? { schedule: trimmed("schedule") } : {}),
    ...(changed("zone") ? { timeZone: trimmed("zone") } : {}),
    ...(current.command != null && changed("command") ? { command: trimmed("command") } : {}),
    ...(changed("concurrency") ? { concurrency: values.concurrency } : {}),
    ...(changed("keepSucceeded") ? { keepSucceeded: Number(values.keepSucceeded) } : {}),
    ...(changed("keepFailed") ? { keepFailed: Number(values.keepFailed) } : {}),
  }
  return Object.keys(edit).length > 2 ? edit : undefined
}

function ScheduleForm({
  cronJob,
  current,
  disabled,
}: {
  cronJob: string
  current: Schemas.TaskSchedule
  disabled: boolean
}) {
  const t = useT()
  const [values, setValues] = useState(() => valuesOf(current))
  const set = (key: keyof ScheduleValues) => (value: string) =>
    setValues((v) => ({ ...v, [key]: value }))
  const shell = current.command != null
  const problems = {
    ...taskProblems(t, { ...values, name: "task", command: shell ? values.command : "-" }),
    name: undefined,
  }
  const edit = disabled ? undefined : scheduleEdit(cronJob, current, values)
  useHold(
    "schedule",
    edit
      ? {
          changes: [edit],
          blocked: Object.values(problems).some(Boolean) ? t("tasks.scheduleBlocked") : undefined,
          scope: { ops: ["editTask"], workload: { kind: "CronJob", name: cronJob } },
        }
      : undefined,
  )
  const shown = (message?: string) => (message ? [message] : [])
  const words = describeCron(t, zoned(values.schedule.trim(), values.zone.trim()))
  return (
    <form
      aria-label={t("part.schedule")}
      className="flex flex-col gap-4 rounded-xl border border-v2-border bg-card p-5"
      onSubmit={(event) => event.preventDefault()}
    >
      <div className="grid gap-4 sm:grid-cols-2">
        <FormField
          label={t("tasks.schedule")}
          hint={words ?? t("tasks.scheduleHint")}
          errors={shown(problems.schedule)}
        >
          <Input
            size="sm"
            mono
            value={values.schedule}
            disabled={disabled}
            autoComplete="off"
            onChange={(event) => set("schedule")(event.target.value)}
          />
        </FormField>
        <FormField label={t("tasks.zone")} hint={t("tasks.zoneHint")}>
          <Input
            size="sm"
            mono
            value={values.zone}
            disabled={disabled}
            placeholder="Etc/UTC"
            autoComplete="off"
            onChange={(event) => set("zone")(event.target.value)}
          />
        </FormField>
      </div>
      {shell ? (
        <FormField
          label={t("tasks.command")}
          hint={t("tasks.commandHint")}
          errors={shown(problems.command)}
        >
          <Input
            size="sm"
            mono
            value={values.command}
            disabled={disabled}
            autoComplete="off"
            onChange={(event) => set("command")(event.target.value)}
          />
        </FormField>
      ) : null}
      <div className="grid gap-4 sm:grid-cols-2">
        <RunFields
          values={values}
          problems={problems}
          disabled={disabled}
          onChange={(key, value) => set(key)(value)}
        />
      </div>
    </form>
  )
}

/** What happens when a run is still going, and how many runs are kept. */
function RunFields({
  values,
  problems,
  disabled,
  onChange,
}: {
  values: Pick<ScheduleValues, "concurrency" | "keepSucceeded" | "keepFailed">
  problems: { keepSucceeded?: string; keepFailed?: string }
  disabled?: boolean
  onChange: (key: "concurrency" | "keepSucceeded" | "keepFailed", value: string) => void
}) {
  const t = useT()
  const shown = (message?: string) => (message ? [message] : [])
  const keep = (key: "keepSucceeded" | "keepFailed") => (
    <FormField label={t(`tasks.${key}`)} errors={shown(problems[key])}>
      <Input
        size="sm"
        inputMode="numeric"
        value={values[key]}
        disabled={disabled}
        onChange={(event) => onChange(key, event.target.value.trim())}
      />
    </FormField>
  )
  return (
    <>
      <FormField label={t("tasks.overlap")}>
        <Select
          size="sm"
          value={values.concurrency}
          disabled={disabled}
          onValueChange={(value) => onChange("concurrency", value)}
          items={(["forbid", "allow", "replace"] as const).map((value) => ({
            value,
            label: t(`tasks.overlap.${value}`),
          }))}
        />
      </FormField>
      <div className="grid grid-cols-2 gap-4">
        {keep("keepSucceeded")}
        {keep("keepFailed")}
      </div>
    </>
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
