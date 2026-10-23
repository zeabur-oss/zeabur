import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { useState } from "react"
import PlayIcon from "~icons/ph/play"
import PlusIcon from "~icons/ph/plus"

import { projectQuery, taskRunsQuery } from "@/api/queries"
import type { Schemas } from "@/api/client"
import { useLogsReadable } from "@/api/logs-access"
import { Ago } from "@/components/bits"
import { FormField } from "@/components/form-field"
import { GateNote, GateTip, useGate } from "@/components/gate"
import { KubeDetails } from "@/components/kube-details"
import { LoadingRows } from "@/components/page"
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

type Concurrency = Schemas["TaskConcurrency"]
type Run = Schemas["TaskRun"]

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

function TaskWhen({ schedule }: { schedule: Schemas["TaskSchedule"] }) {
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
        <FormField label={t("tasks.overlap")}>
          <Select
            size="sm"
            value={concurrency}
            onValueChange={setConcurrency}
            items={(["forbid", "allow", "replace"] as const).map((value) => ({
              value,
              label: t(`tasks.overlap.${value}`),
            }))}
          />
        </FormField>
        <div className="grid grid-cols-2 gap-4">
          <FormField label={t("tasks.keepSucceeded")} errors={shown(problems.keepSucceeded)}>
            <Input
              size="sm"
              inputMode="numeric"
              value={keepSucceeded}
              onChange={(event) => setKeepSucceeded(event.target.value.trim())}
            />
          </FormField>
          <FormField label={t("tasks.keepFailed")} errors={shown(problems.keepFailed)}>
            <Input
              size="sm"
              inputMode="numeric"
              value={keepFailed}
              onChange={(event) => setKeepFailed(event.target.value.trim())}
            />
          </FormField>
        </div>
        <KubeDetails
          facts={[
            [t("tasks.kube.object"), `CronJob ${name} → Job → Pod`],
            [t("tasks.kube.command"), `/bin/sh -c ${JSON.stringify(command.trim())}`],
            [t("tasks.kube.copied"), t("tasks.kube.copiedValue")],
          ]}
        />
      </ReviewForm>
    </PlanDrawer>
  )
}

const RUNS_SHOWN = 10

/** A CronJob's schedule, Run now, and its runs with their logs. */
export function TaskRunsCard({ project, service }: { project: string; service: Service }) {
  const t = useT()
  const gate = useGate(project, "changeServices")
  const planner = usePlanner(project, undefined, true)
  const runs = useQuery(taskRunsQuery(project, service.name))
  return (
    <Container
      title={
        <span className="flex min-w-0 items-center gap-3">
          {t("tasks.runs")}
          {service.schedule ? <TaskWhen schedule={service.schedule} /> : null}
        </span>
      }
      titleAction={
        <GateTip gate={gate}>
          <Button
            variant="outline"
            size="sm"
            disabled={!gate.open}
            onClick={() =>
              planner.plan({
                title: t("tasks.runTitle", service.name),
                changes: [{ op: "runTask", cronJob: service.name }],
                service: { kind: service.kind, name: service.name },
              })
            }
          >
            <PlayIcon aria-hidden="true" />
            {t("tasks.runNow")}
          </Button>
        </GateTip>
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
