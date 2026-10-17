// Backups of a project's claims, databases and namespace, from every backup provider the server
// has (K8up, Velero, CloudNativePG; `GET /projects/{p}/backups`). Shared by the Storage tab and the
// project Overview: one row per target with what schedules it and how its latest backup went, the
// schedules and runs behind a disclosure, and honest notes when a provider is missing or refused.
import { Link } from "@tanstack/react-router"
import { useQuery } from "@tanstack/react-query"
import CaretRightIcon from "~icons/ph/caret-right"

import type { Schemas } from "@/api/client"
import { backupsQuery } from "@/api/queries"
import { Ago } from "@/components/bits"
import { StatusPill } from "@/components/status"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import { useT } from "@/i18n"
import type { Health } from "@/model/service"

type Outcome = Schemas["BackupOutcome"]
export type Coverage = Schemas["BackupCoverage"]

const outcomeHealth: Record<Outcome, Health> = {
  succeeded: "ok",
  partiallyFailed: "warn",
  failed: "fail",
  running: "progress",
  unknown: "idle",
}

const row = "flex items-center gap-2.5 border-t border-rule py-2.5"

/** The project's backups, read where they are shown. */
export function useBackups(project: string) {
  return useQuery(backupsQuery(project))
}

function providerOf(scheduledBy: string) {
  return scheduledBy.split(":")[0] ?? scheduledBy
}

/** The rows worth showing: every claim and database, and the namespace once something backs it
 * up (a namespace row with nothing to say would only repeat the provider note). */
export function shownCoverage(coverage: Coverage[]) {
  return coverage.filter(
    (c) => c.target.kind !== "namespace" || c.scheduledBy.length > 0 || c.lastRun,
  )
}

/** One target: its name, what schedules it, and its latest run. */
export function CoverageRow({ coverage }: { coverage: Coverage }) {
  const t = useT()
  const run = coverage.lastRun
  const providers = [...new Set(coverage.scheduledBy.map(providerOf))]
  return (
    <li className={row}>
      <div className="flex min-w-0 flex-1 flex-col">
        <span className="truncate type-mono" translate="no">
          {coverage.target.kind === "namespace" ? t("backups.namespace") : coverage.target.name}
        </span>
        <span className="truncate type-meta text-muted-foreground">
          {providers.length > 0 ? providers.join(", ") : t("backups.unscheduled")}
        </span>
      </div>
      {run ? (
        <span className="flex shrink-0 items-center gap-2">
          <StatusPill health={outcomeHealth[run.outcome]} size="sm">
            {t(`backups.${run.outcome}`)}
          </StatusPill>
          <Ago iso={run.finished ?? run.started} className="type-meta text-muted-foreground" />
        </span>
      ) : coverage.target.kind !== "namespace" && providers.length === 0 ? (
        <StatusPill health="warn" size="sm" dot={false}>
          {t("backups.none")}
        </StatusPill>
      ) : (
        <span className="shrink-0 type-meta text-muted-foreground">{t("backups.noRun")}</span>
      )}
    </li>
  )
}

/** Notes for providers that are missing or refused, one line each. */
export function ProviderNotes({ data }: { data: Schemas["ProjectBackups"] }) {
  const t = useT()
  const installed = data.providers.filter((p) => p.state === "installed")
  const refused = data.providers.filter((p) => p.state === "refused")
  return (
    <>
      {installed.length === 0 && refused.length === 0 ? (
        <p className="border-t border-rule py-2.5 type-meta text-muted-foreground">
          {t("backups.noProvider")}{" "}
          <Link to="/cluster/components" className="text-foreground hover:underline">
            {t("backups.seeComponents")}
          </Link>
        </p>
      ) : null}
      {refused.map((p) => (
        <p key={p.module} className="border-t border-rule py-2.5 type-meta text-muted-foreground">
          {t("backups.refused", p.module)}
        </p>
      ))}
    </>
  )
}

/** The schedules and runs behind the rows, folded. */
export function BackupDetails({ data }: { data: Schemas["ProjectBackups"] }) {
  const t = useT()
  if (data.schedules.length === 0 && data.runs.length === 0) return null
  return (
    <Collapsible className="mt-1">
      <CollapsibleTrigger variant="plain" className="group/trigger flex items-center">
        <CaretRightIcon
          aria-hidden="true"
          className="size-3.5 transition-transform group-data-[panel-open]/trigger:rotate-90 motion-reduce:transition-none"
        />
        {t("backups.details")}
      </CollapsibleTrigger>
      <CollapsibleContent>
        <ul className="flex flex-col">
          {data.schedules.map((s) => (
            <li key={`${s.provider}:${s.name}`} className={row}>
              <div className="flex min-w-0 flex-1 flex-col">
                <span className="truncate type-mono" translate="no">
                  {s.name}
                </span>
                <span className="truncate type-meta text-muted-foreground">
                  {s.provider}
                  {t("common.sep")}
                  <code translate="no">{s.cron}</code>
                  {s.note
                    ? `${t("common.sep")}${t(s.note as "backups.veleroVolumesNeedSnapshots")}`
                    : ""}
                </span>
              </div>
              {s.paused ? (
                <StatusPill health="idle" size="sm" dot={false}>
                  {t("backups.paused")}
                </StatusPill>
              ) : null}
            </li>
          ))}
          {data.runs.map((r) => (
            <li key={`${r.provider}:${r.name}`} className={row}>
              <div className="flex min-w-0 flex-1 flex-col">
                <span className="truncate type-mono" translate="no">
                  {r.name}
                </span>
                <span
                  className="truncate type-meta text-muted-foreground"
                  title={r.message ?? undefined}
                >
                  {r.provider}
                  {r.message ? `${t("common.sep")}${r.message}` : ""}
                </span>
              </div>
              <StatusPill health={outcomeHealth[r.outcome]} size="sm">
                {t(`backups.${r.outcome}`)}
              </StatusPill>
              <Ago iso={r.finished ?? r.started} className="type-meta text-muted-foreground" />
            </li>
          ))}
        </ul>
      </CollapsibleContent>
    </Collapsible>
  )
}

/** The backups of some claims (a service's), for its Storage tab. */
export function VolumeBackups({ project, claims }: { project: string; claims: string[] }) {
  const t = useT()
  const backups = useBackups(project)
  const data = backups.data
  if (!data || claims.length === 0) return null
  const rows = data.coverage.filter(
    (c) => c.target.kind === "volume" && claims.includes(c.target.name),
  )
  return (
    <section aria-labelledby="storage-backups" className="flex flex-col">
      <h2 id="storage-backups" className="mb-1 flex items-baseline gap-1.5 text-sm font-medium">
        {t("backups.title")}
      </h2>
      <ul className="flex flex-col">
        {rows.map((c) => (
          <CoverageRow key={`${c.target.kind}/${c.target.name}`} coverage={c} />
        ))}
      </ul>
      <ProviderNotes data={data} />
      <BackupDetails data={data} />
    </section>
  )
}
