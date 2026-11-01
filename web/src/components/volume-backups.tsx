import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { useState } from "react"
import ArchiveIcon from "~icons/ph/archive"
import ArrowSquareOutIcon from "~icons/ph/arrow-square-out"
import WarningIcon from "~icons/ph/warning"

import type { Schemas } from "@/api/client"
import { namespaceQuery } from "@/api/queries"
import { BackupDetails, CoverageRow, RunRow, useBackups } from "@/components/backups"
import { FormField } from "@/components/form-field"
import { GateTip, useGate } from "@/components/gate"
import { PlanDrawer, ReviewFooter, ReviewForm } from "@/components/plan/plan-drawer"
import { usePlanner, usePlanStep } from "@/components/plan/planner"
import { FrequencyField, S3Fields } from "@/components/s3-fields"
import { Schedule } from "@/components/schedule"
import { VolumeRestore } from "@/components/volume-restore"
import { type Translate, useT } from "@/i18n"
import {
  backUpVolumeNow,
  backupUser,
  type BackupUser,
  k8upState,
  labelable,
  newVolumeTarget,
  runsOf,
  scheduleOf,
  ownSchedule,
  scheduleVolumeBackups,
  stopVolumeBackups,
  volumeTargetProblems,
  type VolumeTarget,
} from "@/model/volume-backups"
import { Alert, AlertDescription } from "@zeabur-oss/design-system/components/alert"
import { Button } from "@zeabur-oss/design-system/components/button"
import { Input } from "@zeabur-oss/design-system/components/input"

type ProjectBackups = Schemas.ProjectBackups
type Service = Schemas.Service

const userNote = (t: Translate, user: BackupUser) =>
  user.root
    ? t("backups.volume.root")
    : user.user === undefined
      ? t("backups.volume.imageUser")
      : t("backups.volume.user", user.user)

const K8UP_COMPONENT = "volume-backup/k8up"
const RUNS_SHOWN = 5
const BACKUP_COMMAND = "https://docs.k8up.io/k8up/2.16/how-tos/application-aware-backups.html"

/** The backups of a service's volumes, with K8up's schedule and on-demand backup per volume. */
export function VolumeBackups({
  project,
  service,
  claims,
  mounts,
}: {
  project: string
  service: Service
  claims: string[]
  /** Each claim's mount, for a new volume like it. */
  mounts?: Record<string, Schemas.Mount>
}) {
  const t = useT()
  const data = useBackups(project).data
  if (!data || claims.length === 0) return null
  const state = k8upState(data)
  const rows = data.coverage.filter(
    (c) => c.target.kind === "volume" && claims.includes(c.target.name),
  )
  const runs = data.runs
    .filter((r) => claims.some((c) => runsOf(data, c).includes(r)))
    .slice(0, RUNS_SHOWN)
  return (
    <section aria-labelledby="storage-backups" className="flex flex-col">
      <h2 id="storage-backups" className="mb-1 flex items-baseline gap-1.5 text-sm font-medium">
        {t("backups.title")}
      </h2>
      <ul className="flex flex-col">
        {rows.map((c) => (
          <ClaimRow
            key={c.target.name}
            project={project}
            data={data}
            coverage={c}
            installed={state === "installed"}
            user={backupUser(service)}
          />
        ))}
      </ul>
      {state === "not-installed" ? (
        <p className="border-t border-rule py-2.5 type-meta text-muted-foreground">
          {t("backups.volume.notInstalled")}
          {t("common.gap")}
          <Link
            to="/cluster/components"
            search={{ component: K8UP_COMPONENT }}
            className="text-foreground hover:underline"
          >
            {t("backups.volume.openK8up")}
          </Link>
        </p>
      ) : state === "refused" ? (
        <p className="border-t border-rule py-2.5 type-meta text-muted-foreground">
          {t("backups.refused", "k8up")}
        </p>
      ) : null}
      {runs.length > 0 ? (
        <>
          <h3 className="mt-4 mb-1 type-meta text-muted-foreground">{t("backups.volume.runs")}</h3>
          <ul className="flex flex-col">
            {runs.map((r) => (
              <RunRow key={`${r.provider}:${r.name}`} run={r} />
            ))}
          </ul>
        </>
      ) : null}
      {claims
        .filter((c) => scheduleOf(data, c))
        .map((c) => (
          <VolumeRestore
            key={c}
            project={project}
            service={service}
            claim={c}
            mount={mounts?.[c]}
          />
        ))}
      <BackupDetails data={data} />
    </section>
  )
}

function ClaimRow({
  project,
  data,
  coverage,
  installed,
  user,
}: {
  project: string
  data: ProjectBackups
  coverage: Schemas.BackupCoverage
  installed: boolean
  user: BackupUser
}) {
  const t = useT()
  const claim = coverage.target.name
  const schedule = scheduleOf(data, claim)
  const from = data.keysFrom[`${claim}-backup`]
  const keeps = Object.entries(schedule?.retention ?? {})
    .filter(([, n]) => n > 0)
    .map(([period, n]) => t(`backups.volume.keep.${period}` as "backups.volume.keep.last", n))
  const detail = schedule ? (
    <>
      <Schedule cron={schedule.cron} />
      {keeps.length > 0
        ? `${t("common.sep")}${t("backups.volume.keeps", t("common.list", keeps))}`
        : ""}
      {t("common.sep")}
      {schedule.provider}
      {from ? `${t("common.sep")}${t("s3d.from", from)}` : ""}
    </>
  ) : undefined
  const action = schedule ? (
    <span className="flex items-center gap-1.5">
      <BackUpNow project={project} claim={claim} schedule={schedule} />
      {ownSchedule(schedule, claim) ? (
        <StopBackups project={project} claim={claim} schedule={schedule} />
      ) : null}
    </span>
  ) : installed ? (
    <SetUpBackups project={project} claim={claim} user={user} />
  ) : null
  return <CoverageRow coverage={coverage} detail={detail} action={action} />
}

function BackUpNow({
  project,
  claim,
  schedule,
}: {
  project: string
  claim: string
  schedule: Schemas.BackupSchedule
}) {
  const t = useT()
  const planner = usePlanner(project)
  const gate = useGate(project, "backUpVolumes")
  if (!schedule.runNow) return null
  return (
    <>
      <GateTip gate={gate}>
        <Button
          variant="outline"
          size="xs"
          onClick={() =>
            planner.plan({
              title: t("backups.volume.planNow", claim),
              changes: backUpVolumeNow(schedule, claim, new Date()),
            })
          }
        >
          <ArchiveIcon aria-hidden="true" data-icon="inline-start" />
          {t("db.backup.now")}
        </Button>
      </GateTip>
      {planner.dialog}
    </>
  )
}

function StopBackups({
  project,
  claim,
  schedule,
}: {
  project: string
  claim: string
  schedule: Schemas.BackupSchedule
}) {
  const t = useT()
  const planner = usePlanner(project)
  const gate = useGate(project, "backUpVolumes")
  return (
    <>
      <GateTip gate={gate}>
        <Button
          variant="ghost"
          size="xs"
          onClick={() =>
            planner.plan({
              title: t("backups.volume.planStop", claim),
              changes: stopVolumeBackups(schedule),
              summary: {
                lines: [t("backups.volume.stopSaid", claim, `${claim}-backup`)],
                action: t("backups.volume.stop"),
              },
            })
          }
        >
          {t("backups.volume.stop")}
        </Button>
      </GateTip>
      {planner.dialog}
    </>
  )
}

function SetUpBackups({
  project,
  claim,
  user,
}: {
  project: string
  claim: string
  user: BackupUser
}) {
  const t = useT()
  const [open, setOpen] = useState(false)
  const gate = useGate(project, "backUpVolumes")
  if (!labelable(claim))
    return (
      <span className="w-full type-meta text-muted-foreground">{t("backups.volume.long")}</span>
    )
  return (
    <>
      <GateTip gate={gate}>
        <Button variant="outline" size="xs" onClick={() => setOpen(true)}>
          <ArchiveIcon aria-hidden="true" data-icon="inline-start" />
          {t("backups.volume.set")}
        </Button>
      </GateTip>
      {open ? (
        <ScheduleDrawer
          project={project}
          claim={claim}
          user={user}
          onClose={() => setOpen(false)}
        />
      ) : null}
    </>
  )
}

function ScheduleDrawer({
  project,
  claim,
  user,
  onClose,
}: {
  project: string
  claim: string
  user: BackupUser
  onClose: () => void
}) {
  const t = useT()
  const [target, setTarget] = useState<VolumeTarget>(newVolumeTarget)
  const [tried, setTried] = useState(false)
  const step = usePlanStep(project, onClose)
  const enforced = useQuery(namespaceQuery(project)).data?.podSecurity
  const refused = user.root && enforced === "restricted"
  const problems = volumeTargetProblems(target)
  const set = (patch: Partial<VolumeTarget>) => setTarget((v) => ({ ...v, ...patch }))
  const errors = (problem: (typeof problems)[number]) =>
    tried && problems.includes(problem)
      ? [t(problem === "keep" ? "backups.volume.problem.keep" : `db.backup.problem.${problem}`)]
      : []
  const review = () => {
    setTried(true)
    if (refused || problems.length > 0) return
    step.plan({
      title: t("db.backup.plan", claim),
      changes: scheduleVolumeBackups(project, claim, target, user),
      notes: [t("backups.volume.password", `${claim}-backup`), userNote(t, user)],
    })
  }
  const count = (key: "keepLast" | "keepDays" | "keepWeekly", label: string, max: number) => (
    <FormField label={label} errors={key === "keepDays" ? errors("keep") : []}>
      <Input
        size="sm"
        type="number"
        min={key === "keepDays" ? 1 : 0}
        max={max}
        value={String(target[key])}
        onChange={(event) => set({ [key]: Number(event.target.value) })}
      />
    </FormField>
  )
  return (
    <PlanDrawer
      step={step}
      onClose={onClose}
      title={t("db.backup.plan", claim)}
      description={t("backups.volume.body")}
      footer={<ReviewFooter onCancel={onClose} onReview={review} disabled={refused} />}
    >
      <ReviewForm onReview={review}>
        {refused ? (
          <Alert variant="warn">
            <WarningIcon aria-hidden="true" />
            <AlertDescription>
              {t("backups.volume.restricted")}
              {t("common.gap")}
              <a
                href="#security"
                onClick={(event) => {
                  event.preventDefault()
                  onClose()
                  document.getElementById("security")?.scrollIntoView({ block: "start" })
                  history.replaceState(history.state, "", "#security")
                }}
                className="text-foreground hover:underline"
              >
                {t("backups.volume.openSecurity")}
              </a>
            </AlertDescription>
          </Alert>
        ) : (
          <p className="type-meta text-muted-foreground">{userNote(t, user)}</p>
        )}
        <p className="type-meta text-muted-foreground">
          {t("backups.volume.liveFiles")}
          {t("common.gap")}
          <a
            href={BACKUP_COMMAND}
            target="_blank"
            rel="noreferrer"
            className="inline-flex items-center gap-1 text-foreground hover:underline"
          >
            <span translate="no">k8up.io/backupcommand</span>
            <ArrowSquareOutIcon aria-hidden="true" className="size-3" />
          </a>
        </p>
        <S3Fields
          target={target}
          set={set}
          errors={errors}
          folder={`${project}/${claim}`}
          folderHint={t("backups.volume.folderHint", `${project}/${claim}`)}
        >
          <FrequencyField value={target.frequency} onChange={(frequency) => set({ frequency })} />
          {count("keepLast", t("backups.volume.keepLast"), 100)}
          {count("keepDays", t("backups.volume.keepDaily"), 365)}
          {count("keepWeekly", t("backups.volume.keepWeekly"), 520)}
        </S3Fields>
      </ReviewForm>
    </PlanDrawer>
  )
}
