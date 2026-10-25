// What deleting a project takes and what stays, shown in its plan. "Back up first" swaps the plan
// for a backup's; the person comes back to delete once it has run.
import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import type { ReactNode } from "react"
import ArchiveIcon from "~icons/ph/archive"
import WarningFillIcon from "~icons/ph/warning-fill"

import type { Schemas } from "@/api/client"
import { databasesQuery } from "@/api/queries"
import { CoverageRow, useBackups, type Coverage } from "@/components/backups"
import { bounded } from "@/components/plan/groups"
import type { PlanAsk } from "@/components/plan/planner"
import { Alert, AlertDescription } from "@zeabur/design-system/components/alert"
import { Button } from "@zeabur/design-system/components/button"
import { useT, type Translate } from "@/i18n"
import { backUpNow, backUpNowSummary, canBackUpNow } from "@/model/databases"
import { dataFate, projectDeletionOf, recentBackup } from "@/model/project-deletion"
import { backUpVolumeNow, scheduleOf } from "@/model/volume-backups"

type DeletesProject = Extract<Schemas["PlanWarning"], { code: "deletesProject" }>
type Target = Schemas["BackupTarget"]

function coverageOf(data: Schemas["ProjectBackups"] | undefined, target: Target): Coverage {
  const found = data?.coverage.find(
    (c) => c.target.kind === target.kind && c.target.name === target.name,
  )
  return found ?? { target, scheduledBy: [] }
}

function fateText(t: Translate, volume: Schemas["VolumeDeletion"]): string {
  switch (dataFate(volume)) {
    case "deleted":
      return t("deletion.dataDeleted")
    case "kept":
      return volume.volume ? t("deletion.dataKept", volume.volume) : t("deletion.dataKeptUnbound")
    case "unknown":
      return t("deletion.dataUnknown")
  }
}

/** The plan's deletion of the project, when it has one. */
export function ProjectDeletion({
  project,
  plan,
  reopen,
}: {
  project: string
  plan: Schemas["PlanResponse"]
  reopen: (ask: PlanAsk) => void
}) {
  const deletion = projectDeletionOf(plan.warnings)
  return deletion ? <Deletion project={project} deletion={deletion} reopen={reopen} /> : null
}

function Deletion({
  project,
  deletion,
  reopen,
}: {
  project: string
  deletion: DeletesProject
  reopen: (ask: PlanAsk) => void
}) {
  const t = useT()
  const backups = useBackups(project).data
  const databases = useQuery(databasesQuery(project)).data?.items ?? []
  const now = new Date()
  const backUp = (ask: PlanAsk) => (
    <Button variant="outline" size="xs" onClick={() => reopen(ask)}>
      <ArchiveIcon aria-hidden="true" data-icon="inline-start" />
      {t("deletion.backUpFirst")}
    </Button>
  )
  const databaseAction = (name: string, coverage: Coverage): ReactNode => {
    const database = databases.find((d) => d.name === name)
    if (recentBackup(coverage.lastRun) || !database) return null
    if (!canBackUpNow(database.engine) || !database.backups.ready) return null
    return backUp({
      title: t("db.backup.planNow", database.name),
      changes: backUpNow(database, now),
      summary: backUpNowSummary(t, database),
    })
  }
  const volumeAction = (claim: string, coverage: Coverage): ReactNode => {
    const schedule = backups ? scheduleOf(backups, claim) : undefined
    if (recentBackup(coverage.lastRun) || !schedule) return null
    const changes = backUpVolumeNow(schedule, claim, now)
    return changes.length > 0 ? backUp({ title: t("deletion.backUpVolume", claim), changes }) : null
  }
  const down = deletion.databases.filter((d) => d.operator === "notReady")
  return (
    <div className="mt-4 flex flex-col gap-4">
      <section className="flex flex-col gap-1" aria-labelledby="deletion-goes">
        <h3 id="deletion-goes" className="type-label">
          {t("deletion.goes")}
        </h3>
        {deletion.services.length > 0 ? (
          <p className="type-meta text-muted-foreground">
            {t(
              "deletion.services",
              bounded(
                t,
                deletion.services.map((s) => s.name),
              ).text,
            )}
          </p>
        ) : null}
        {deletion.databases.length + deletion.volumes.length > 0 ? (
          <ul className="flex flex-col">
            {deletion.databases.map((d) => {
              const coverage = coverageOf(backups, { kind: "database", name: d.name })
              return (
                <CoverageRow
                  key={`${d.kind}/${d.name}`}
                  coverage={coverage}
                  detail={t("deletion.database", d.kind)}
                  action={databaseAction(d.name, coverage)}
                />
              )
            })}
            {deletion.volumes.map((v) => {
              const coverage = coverageOf(backups, { kind: "volume", name: v.claim })
              return (
                <CoverageRow
                  key={v.claim}
                  coverage={coverage}
                  detail={fateText(t, v)}
                  action={volumeAction(v.claim, coverage)}
                />
              )
            })}
          </ul>
        ) : null}
        {deletion.toolsPods.length > 0 ? (
          <p className="type-meta text-muted-foreground">{t("deletion.toolsPods")}</p>
        ) : null}
      </section>
      {down.length > 0 ? (
        <Alert variant="warn" role="note">
          <WarningFillIcon aria-hidden="true" />
          <AlertDescription>
            {t(
              "deletion.operatorDown",
              bounded(
                t,
                down.map((d) => d.name),
              ).text,
            )}
          </AlertDescription>
        </Alert>
      ) : null}
      <section className="flex flex-col gap-1" aria-labelledby="deletion-stays">
        <h3 id="deletion-stays" className="type-label">
          {t("deletion.stays")}
        </h3>
        <p className="type-meta text-muted-foreground">{t("deletion.staysBackups")}</p>
        {backups?.schedules.some((schedule) => schedule.provider === "k8up") ? (
          <p className="type-meta text-muted-foreground">{t("deletion.resticPassword")}</p>
        ) : null}
        <p className="type-meta text-muted-foreground">{t("deletion.staysOutside")}</p>
        <p className="type-meta text-muted-foreground">
          {t("deletion.restore", t("clusterBackups.restoreGoneAction"))}
          {t("common.gap")}
          <Link to="/cluster/backups" className="text-foreground hover:underline">
            {t("deletion.openBackups")}
          </Link>
        </p>
      </section>
    </div>
  )
}
