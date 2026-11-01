// Restoring a backup into a new database, and moving services between it and the original.
import { useQuery } from "@tanstack/react-query"
import { useState } from "react"
import ArrowCounterClockwiseIcon from "~icons/ph/arrow-counter-clockwise"

import { databasesQuery, revealSecret } from "@/api/queries"
import { useTargets } from "@/components/connect"
import { FormField, NameField } from "@/components/form-field"
import { GateTip, useGate } from "@/components/gate"
import { PlanDrawer, ReviewFooter, ReviewForm } from "@/components/plan/plan-drawer"
import { usePlanner, usePlanStep, type PlanStepState } from "@/components/plan/planner"
import { Problem } from "@/components/problem"
import { useI18n, useT, type Locale, type Translate } from "@/i18n"
import { formatTime } from "@/lib/format"
import {
  keptSecret,
  localInput,
  pointInTime,
  pointServices,
  recoveryRange,
  restoreChanges,
  restoredName,
  restoreKin,
  restorePoints,
  type RestorePoint,
  utcInstant,
} from "@/model/database-restore"
import { nameProblem, type Database } from "@/model/databases"
import { Alert, AlertDescription, AlertTitle } from "@zeabur-oss/design-system/components/alert"
import { Button } from "@zeabur-oss/design-system/components/button"
import { Input } from "@zeabur-oss/design-system/components/input"
import { RadioGroup, RadioOption } from "@zeabur-oss/design-system/components/radio"
import { Select } from "@zeabur-oss/design-system/components/select"

/** "Restore into a new database", under the backups. */
export function Restore({ project, database }: { project: string; database: Database }) {
  const t = useT()
  const [open, setOpen] = useState(false)
  const gate = useGate(project, "changeDatabases")
  const points = restorePoints(database)
  const can = points.length > 0 || recoveryRange(database, new Date()) !== undefined
  return (
    <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
      <p className="min-w-0 flex-1 type-meta text-muted-foreground">
        {can ? t("db.restore.body") : t("db.restore.none")}
      </p>
      {can ? (
        <GateTip gate={gate}>
          <Button variant="outline" size="sm" onClick={() => setOpen(true)}>
            <ArrowCounterClockwiseIcon aria-hidden="true" data-icon="inline-start" />
            {t("db.restore.action")}
          </Button>
        </GateTip>
      ) : null}
      {open ? (
        <RestoreDrawer
          project={project}
          database={database}
          points={points}
          onClose={() => setOpen(false)}
        />
      ) : null}
    </div>
  )
}

type Mode = "backup" | "time"

function RestoreDrawer({
  project,
  database,
  points,
  onClose,
}: {
  project: string
  database: Database
  points: RestorePoint[]
  onClose: () => void
}) {
  const [name, setName] = useState(() => restoredName(database.name, new Date()))
  const [made, setMade] = useState<string | undefined>(undefined)
  const step = usePlanStep(project, () => (made ? onClose() : setMade(name)))
  return made ? (
    <MadeStep project={project} database={database} made={made} step={step} onClose={onClose} />
  ) : (
    <PointForm
      project={project}
      database={database}
      points={points}
      name={name}
      onName={setName}
      step={step}
      onClose={onClose}
    />
  )
}

/** After the copy is created: wait until it is ready, then point services to it. */
function MadeStep({
  project,
  database,
  made,
  step,
  onClose,
}: {
  project: string
  database: Database
  made: string
  step: PlanStepState
  onClose: () => void
}) {
  const t = useT()
  const all = useQuery(databasesQuery(project)).data?.items ?? []
  const targets = useTargets(project)
  const copy = all.find((d) => d.name === made)
  const changes = copy?.state === "ready" ? pointServices(database, copy, targets) : []
  const repoint = () =>
    step.plan({
      title: t("db.restored.pointTitle", made),
      changes,
      notes: [t("db.restored.keep", database.name)],
    })
  const status =
    copy?.state !== "ready"
      ? t("db.restore.making", made)
      : changes.length === 0
        ? t("db.restored.noReaders", database.name)
        : t("db.restore.ready", made)
  return (
    <PlanDrawer
      step={step}
      onClose={onClose}
      title={t("db.restore.title", database.name)}
      footer={
        <ReviewFooter
          onCancel={onClose}
          onReview={repoint}
          disabled={changes.length === 0}
          label={t("db.restored.point")}
        />
      }
    >
      <p className="type-body">{status}</p>
      <p className="type-meta text-muted-foreground">{t("db.restored.keep", database.name)}</p>
      {database.engine === "postgresql" ? (
        <p className="type-meta text-muted-foreground">{t("db.restore.download", made)}</p>
      ) : null}
    </PlanDrawer>
  )
}

/** The values of the original's Secret the copy keeps, read as the person; none when it keeps
 * none. */
async function keptValues(project: string, database: Database) {
  const kept = keptSecret(database)
  return kept ? { kept, values: await revealSecret(project, kept.name) } : undefined
}

function PointForm({
  project,
  database,
  points,
  name,
  onName,
  step,
  onClose,
}: {
  project: string
  database: Database
  points: RestorePoint[]
  name: string
  onName: (name: string) => void
  step: PlanStepState
  onClose: () => void
}) {
  const t = useT()
  const { locale } = useI18n()
  const { range, mode, setMode, backup, setBackup, at, setAt, point, outside } = usePoint(
    database,
    points,
  )
  const problem = useNameProblem(project, name)
  const plan = async (chosen: RestorePoint) => {
    const copied = await keptValues(project, database)
    step.plan({
      title: t("db.restore.plan", database.name, name),
      changes: restoreChanges(database, chosen, name, project, undefined, copied?.values),
      notes: restoreNotes(t, locale, database.name, chosen, name, copied?.kept),
    })
  }
  const { tried, failure, busy, review } = useReview(() =>
    problem || !point ? undefined : plan(point),
  )
  return (
    <PlanDrawer
      step={step}
      onClose={onClose}
      title={t("db.restore.title", database.name)}
      description={t("db.restore.body")}
      footer={<ReviewFooter onCancel={onClose} onReview={review} disabled={busy} />}
    >
      <ReviewForm onReview={review}>
        <ModeField mode={mode} onMode={setMode} range={range} backups={points.length > 0} />
        {mode === "backup" ? (
          <BackupField points={points} value={backup} onChange={setBackup} />
        ) : (
          <TimeField
            range={range}
            at={at}
            onAt={setAt}
            error={tried && outside ? t("db.restore.outside") : undefined}
          />
        )}
        <NameField
          label={t("db.restore.name")}
          error={tried && problem ? t(`db.name.${problem}`) : undefined}
          value={name}
          onChange={onName}
        />
        <p className="type-meta text-muted-foreground">{t(AS_OF[mode])}</p>
        {failure ? <Problem error={failure} /> : null}
      </ReviewForm>
    </PlanDrawer>
  )
}

type Range = { from: Date; to: Date }

const AS_OF = { backup: "db.restore.asOf", time: "db.restore.asOfTime" } as const satisfies Record<
  Mode,
  string
>

function useNameProblem(project: string, name: string) {
  const all = useQuery(databasesQuery(project)).data?.items ?? []
  return nameProblem(
    name,
    all.map((d) => d.name),
  )
}

/** Planning once the form is valid: `start` returns nothing while it is not. */
function useReview(start: () => Promise<void> | undefined) {
  const [tried, setTried] = useState(false)
  const [failure, setFailure] = useState<unknown>(undefined)
  const [busy, setBusy] = useState(false)
  const review = () => {
    setTried(true)
    setFailure(undefined)
    if (busy) return
    const started = start()
    if (!started) return
    setBusy(true)
    started.catch(setFailure).finally(() => setBusy(false))
  }
  return { tried, failure, busy, review }
}

/** The point chosen: a finished backup, or a time inside the recovery window. */
function usePoint(database: Database, points: RestorePoint[]) {
  const [range] = useState(() => recoveryRange(database, new Date()))
  const [mode, setMode] = useState<Mode>(points.length > 0 ? "backup" : "time")
  const [backup, setBackup] = useState(points[0]?.backup ?? "")
  const [at, setAt] = useState(() => (range ? localInput(range.to) : ""))
  const time = at ? new Date(at) : undefined
  const outside = mode === "time" && !insideRange(time, range)
  const point = mode === "time" ? timePoint(time, range) : points.find((p) => p.backup === backup)
  return { range, mode, setMode, backup, setBackup, at, setAt, point, outside }
}

const insideRange = (time: Date | undefined, range: Range | undefined) =>
  !!time && !!range && time >= range.from && time <= range.to

const timePoint = (time: Date | undefined, range: Range | undefined) =>
  time && insideRange(time, range) ? pointInTime(time) : undefined

function restoreNotes(
  t: Translate,
  locale: Locale,
  from: string,
  chosen: RestorePoint,
  name: string,
  kept: { name: string; suffix: string } | undefined,
) {
  const instant = chosen.target
    ? t("db.restore.instant", formatTime(chosen.target, locale), chosen.target)
    : t("db.restore.asOf")
  const copied = kept ? [t("db.restore.copiedSecret", kept.name, `${name}-${kept.suffix}`)] : []
  return [t("db.restore.unchanged", from), instant, ...copied]
}

function BackupField({
  points,
  value,
  onChange,
}: {
  points: RestorePoint[]
  value: string
  onChange: (backup: string) => void
}) {
  const t = useT()
  const { locale } = useI18n()
  return (
    <FormField label={t("db.restore.point")}>
      <Select
        size="sm"
        className="max-w-full"
        value={value}
        onValueChange={onChange}
        translate="no"
        items={points.map((p) => ({ value: p.backup, label: pointLabel(t, locale, p) }))}
      />
    </FormField>
  )
}

function pointLabel(t: Translate, locale: Locale, p: RestorePoint) {
  if (p.backup === "latest") return t("db.restore.latest")
  return p.time ? `${formatTime(p.time, locale)}${t("common.sep")}${p.backup}` : p.backup
}

/** From a finished backup, or from a time inside the recovery window, when there is one. */
function ModeField({
  mode,
  onMode,
  range,
  backups,
}: {
  mode: Mode
  onMode: (mode: Mode) => void
  range: Range | undefined
  backups: boolean
}) {
  const t = useT()
  const { locale } = useI18n()
  if (!range) return null
  return (
    <RadioGroup value={mode} onValueChange={(value) => onMode(value as Mode)}>
      <RadioOption value="backup" disabled={!backups} label={t("db.restore.fromBackup")} />
      <RadioOption
        value="time"
        label={t("db.restore.fromTime")}
        description={t(
          "db.restore.window",
          formatTime(range.from.toISOString(), locale),
          formatTime(range.to.toISOString(), locale),
        )}
      />
    </RadioGroup>
  )
}

/** A time in the viewer's zone inside the recovery window, with its UTC instant. */
function TimeField({
  range,
  at,
  onAt,
  error,
}: {
  range: { from: Date; to: Date } | undefined
  at: string
  onAt: (at: string) => void
  error?: string
}) {
  const t = useT()
  const time = at ? new Date(at) : undefined
  return (
    <FormField
      label={t("db.restore.time")}
      hint={time && !error ? t("db.restore.utc", utcInstant(time)) : undefined}
      errors={error ? [error] : []}
    >
      <Input
        size="sm"
        type="datetime-local"
        step={1}
        min={range ? localInput(range.from) : undefined}
        max={range ? localInput(range.to) : undefined}
        value={at}
        onChange={(event) => onAt(event.target.value)}
      />
    </FormField>
  )
}

/** Where this database came from, or what came from it, with moving services over. */
export function Restored({ project, database }: { project: string; database: Database }) {
  const all = useQuery(databasesQuery(project)).data?.items ?? []
  const { source, copies } = restoreKin(database, all)
  return (
    <>
      {database.restored ? (
        <Kin
          project={project}
          here={database}
          other={source}
          otherName={database.restored.from}
          time={database.restored.time}
          copy={false}
        />
      ) : null}
      {copies
        .filter((c) => c.state === "ready")
        .map((copy) => (
          <Kin
            key={copy.name}
            project={project}
            here={database}
            other={copy}
            otherName={copy.name}
            time={copy.restored?.time}
            copy
          />
        ))}
    </>
  )
}

function Kin({
  project,
  here,
  other,
  otherName,
  time,
  copy,
}: {
  project: string
  here: Database
  other?: Database
  otherName: string
  time?: string | null
  /** `other` was restored from `here`. */
  copy: boolean
}) {
  const t = useT()
  const { locale } = useI18n()
  const planner = usePlanner(project)
  const gate = useGate(project, "changeServices")
  const targets = useTargets(project)
  const changes = other ? pointServices(other, here, targets) : []
  const when = time ? formatTime(time, locale) : "—"
  const services = [
    ...new Set(changes.map((c) => (c.op === "env" ? (c.workload?.name ?? "") : ""))),
  ]
  const title = copy
    ? t("db.restored.copy", otherName, when)
    : t("db.restored.from", otherName, when)
  const plan = () =>
    planner.plan({
      title: t("db.restored.pointTitle", here.name),
      changes,
      notes: [t("db.restored.keep", otherName)],
    })
  return (
    <Alert variant="info" role="note">
      <ArrowCounterClockwiseIcon aria-hidden="true" />
      <AlertTitle>{title}</AlertTitle>
      <AlertDescription>
        <p>
          {here.state !== "ready"
            ? t("db.restored.waiting", here.name)
            : changes.length === 0
              ? t("db.restored.noReaders", otherName)
              : t("db.restored.readers", otherName, t("common.list", services))}
        </p>
        {here.state === "ready" && changes.length > 0 ? (
          <GateTip gate={gate}>
            <Button variant="secondary" size="sm" onClick={plan}>
              {t("db.restored.point")}
            </Button>
          </GateTip>
        ) : null}
      </AlertDescription>
      {planner.dialog}
    </Alert>
  )
}
