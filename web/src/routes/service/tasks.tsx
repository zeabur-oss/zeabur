import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { useState } from "react"
import PlusIcon from "~icons/ph/plus"

import { projectQuery } from "@/api/queries"
import type { Schemas } from "@/api/client"
import { FormField } from "@/components/form-field"
import { GateNote, GateTip, useGate } from "@/components/gate"
import { useHold } from "@/components/plan/draft"
import { PlanDrawer, ReviewFooter, ReviewForm } from "@/components/plan/plan-drawer"
import { usePlanStep } from "@/components/plan/planner"
import { useT, type Translate } from "@/i18n"
import { describeCron } from "@/lib/cron"
import { serviceLink } from "@/lib/place"
import type { Service } from "@/model/service"
import { ContainerField } from "@/routes/service/container-field"
import { TaskWhen, zoned } from "@/routes/service/task-runs"
import { Button } from "@zeabur-oss/design-system/components/button"
import { Card, CardRow } from "@zeabur-oss/design-system/components/card"
import { Input } from "@zeabur-oss/design-system/components/input"
import { SectionHeader } from "@zeabur-oss/design-system/components/section-header"
import { Select } from "@zeabur-oss/design-system/components/select"

type Concurrency = Schemas.TaskConcurrency

const NAME = /^[a-z0-9]([-a-z0-9]*[a-z0-9])?$/
const NAME_MAX = 52

const keepable = (value: string) => /^\d+$/.test(value) && Number(value) <= 100

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
