// Restoring a backup into a new database, and moving services between it and the original.
import { useQuery } from "@tanstack/react-query"
import { useState } from "react"
import ArrowCounterClockwiseIcon from "~icons/ph/arrow-counter-clockwise"

import { databasesQuery } from "@/api/queries"
import { useTargets } from "@/components/connect"
import { FormField, NameField } from "@/components/form-field"
import { GateTip, useGate } from "@/components/gate"
import { PlanDrawer, ReviewFooter, ReviewForm } from "@/components/plan/plan-drawer"
import { usePlanner, usePlanStep } from "@/components/plan/planner"
import { useI18n, useT } from "@/i18n"
import { formatDate } from "@/lib/format"
import {
  pointServices,
  restoreChanges,
  restoredName,
  restoreKin,
  restorePoints,
  type RestorePoint,
} from "@/model/database-restore"
import { nameProblem, type Database } from "@/model/databases"
import { Alert, AlertDescription, AlertTitle } from "@zeabur/design-system/components/alert"
import { Button } from "@zeabur/design-system/components/button"
import { Select } from "@zeabur/design-system/components/select"

/** "Restore into a new database", under the backups. */
export function Restore({ project, database }: { project: string; database: Database }) {
  const t = useT()
  const [open, setOpen] = useState(false)
  const gate = useGate(project, "changeDatabases")
  const points = restorePoints(database)
  return (
    <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
      <p className="min-w-0 flex-1 type-meta text-muted-foreground">
        {points.length > 0 ? t("db.restore.body") : t("db.restore.none")}
      </p>
      {points.length > 0 ? (
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
  const t = useT()
  const { locale } = useI18n()
  const taken = useQuery(databasesQuery(project)).data?.items.map((d) => d.name) ?? []
  const [backup, setBackup] = useState(points[0]?.backup ?? "")
  const [name, setName] = useState(() => restoredName(database.name, new Date()))
  const [tried, setTried] = useState(false)
  const step = usePlanStep(project, onClose)
  const problem = nameProblem(name, taken)
  const point = points.find((p) => p.backup === backup)
  const label = (p: RestorePoint) =>
    p.backup === "latest"
      ? t("db.restore.latest")
      : p.time
        ? `${formatDate(p.time, locale)}${t("common.sep")}${p.backup}`
        : p.backup
  const review = () => {
    setTried(true)
    if (problem || !point) return
    const users = (database.object as { spec?: { secrets?: { users?: string } } }).spec?.secrets
      ?.users
    step.plan({
      title: t("db.restore.plan", database.name, name),
      changes: restoreChanges(database, point, name, project),
      notes: [
        t("db.restore.unchanged", database.name),
        t("db.restore.asOf"),
        ...(database.engine === "mongodb"
          ? [t("db.restore.sharedSecret", users ?? `${database.name}-secrets`)]
          : []),
      ],
    })
  }
  return (
    <PlanDrawer
      step={step}
      onClose={onClose}
      title={t("db.restore.title", database.name)}
      description={t("db.restore.body")}
      footer={<ReviewFooter onCancel={onClose} onReview={review} />}
    >
      <ReviewForm onReview={review}>
        <FormField label={t("db.restore.point")}>
          <Select
            size="sm"
            className="max-w-full"
            value={backup}
            onValueChange={setBackup}
            translate="no"
            items={points.map((p) => ({ value: p.backup, label: label(p) }))}
          />
        </FormField>
        <NameField
          label={t("db.restore.name")}
          error={tried && problem ? t(`db.name.${problem}`) : undefined}
          value={name}
          onChange={setName}
        />
        <p className="type-meta text-muted-foreground">{t("db.restore.asOf")}</p>
      </ReviewForm>
    </PlanDrawer>
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
  const when = time ? formatDate(time, locale) : "—"
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
