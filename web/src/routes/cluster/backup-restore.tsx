// A Velero backup restored into a new project while the original runs, or under its own name
// once the original is gone; then the domains and the addresses moved over.
import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { useState } from "react"
import WarningIcon from "~icons/ph/warning"

import type { Schemas } from "@/api/client"
import { configMapsQuery, objectsQuery, projectsQuery, revealSecret } from "@/api/queries"
import { Ago } from "@/components/bits"
import { useTargets } from "@/components/connect"
import { FormField, NameField } from "@/components/form-field"
import { GateTip, useGate } from "@/components/gate"
import { usePlanner } from "@/components/plan/planner"
import { Problem } from "@/components/problem"
import { StatusPill } from "@/components/status"
import { useT } from "@/i18n"
import { useQuantityFormat } from "@/lib/quantity"
import { outcomeHealth } from "@/model/cluster-backups"
import { restoredName } from "@/model/database-restore"
import { ownSecrets, restoreChoices, restoreProblem, rewriteChanges } from "@/model/restore-copy"
import type { Gate } from "@/model/rights"
import { FormButtons, Section } from "@/routes/cluster/section"
import { Alert, AlertDescription } from "@zeabur/design-system/components/alert"
import { Button } from "@zeabur/design-system/components/button"
import { Select } from "@zeabur/design-system/components/select"

type Request = Schemas["BackupsPlanRequest"]
type OnPlan = (request: Request, title: string, action?: string) => void
type Copy = Schemas["RestoreCopy"]

const NAMESPACE_MAX = 63

/** The project to restore and, while it exists, the new project's name. */
export function RestoreForm({
  backup,
  gate,
  onCancel,
  onPlan,
}: {
  backup: Schemas["VeleroBackup"]
  gate: Gate
  onCancel: () => void
  onPlan: OnPlan
}) {
  const t = useT()
  const projects = useQuery(projectsQuery).data?.items ?? []
  const choices = restoreChoices(backup.namespaces, projects)
  const [namespace, setNamespace] = useState(choices.length === 1 ? (choices[0] ?? "") : "")
  const [typed, setTyped] = useState<string | undefined>(undefined)
  const [tried, setTried] = useState(false)
  const exists = projects.some((p) => p.name === namespace)
  const into = typed ?? (namespace ? restoredName(namespace, new Date(), NAMESPACE_MAX) : "")
  const problem = restoreProblem(namespace, into, projects)
  return (
    <form
      className="mt-2 flex flex-col gap-4 rounded-xl border border-main bg-card p-5"
      onSubmit={(event) => {
        event.preventDefault()
        setTried(true)
        if (problem) return
        onPlan(
          { op: "restore", backup: backup.name, namespace, ...(exists ? { into } : {}) },
          t("clusterBackups.plan.copy", namespace),
          exists ? t("clusterBackups.copyAction") : t("clusterBackups.restoreAction"),
        )
      }}
    >
      <FormField
        label={t("clusterBackups.restoreFrom")}
        errors={tried && problem === "restoreNone" ? [t("clusterBackups.problem.restoreNone")] : []}
      >
        <Select
          size="sm"
          className="max-w-full"
          value={namespace}
          onValueChange={(value) => {
            setNamespace(value)
            setTyped(undefined)
          }}
          translate="no"
          items={[
            ...(namespace ? [] : [{ value: "", label: t("clusterBackups.chooseProject") }]),
            ...choices.map((name) => ({ value: name, label: name })),
          ]}
        />
      </FormField>
      {namespace ? (
        <IntoField
          namespace={namespace}
          exists={exists}
          into={into}
          taken={tried && problem === "intoTaken"}
          onChange={setTyped}
        />
      ) : null}
      <FormButtons gate={gate} onCancel={onCancel} />
    </form>
  )
}

/** The new project's name while the original exists; else a note that it keeps its own. */
function IntoField({
  namespace,
  exists,
  into,
  taken,
  onChange,
}: {
  namespace: string
  exists: boolean
  into: string
  taken: boolean
  onChange: (name: string) => void
}) {
  const t = useT()
  if (!exists)
    return (
      <p className="type-meta text-muted-foreground">{t("clusterBackups.goneBody", namespace)}</p>
    )
  return (
    <NameField
      label={t("clusterBackups.copyName")}
      hint={t("clusterBackups.copyHint", namespace)}
      error={taken ? t("clusterBackups.problem.intoTaken") : undefined}
      value={into}
      onChange={onChange}
    />
  )
}

function Room({ room }: { room: Schemas["RestoreRoom"] }) {
  const t = useT()
  const { memory, cores } = useQuantityFormat()
  const need = t("clusterBackups.copy.amount", cores(room.needCpu), memory(room.needMemory))
  const free = t("clusterBackups.copy.amount", cores(room.freeCpu), memory(room.freeMemory))
  if (room.fit === "short")
    return (
      <Alert variant="warn" role="note">
        <WarningIcon aria-hidden="true" />
        <AlertDescription>{t("clusterBackups.copy.short", need, free)}</AlertDescription>
      </Alert>
    )
  return (
    <p className="type-meta text-muted-foreground">
      {room.fit === "fits"
        ? t("clusterBackups.copy.fits", need, free)
        : t("clusterBackups.copy.roomUnknown", need)}
    </p>
  )
}

/** What a restore plan does to the projects, under the writes. */
export function CopyFacts({ copy }: { copy: Copy }) {
  const t = useT()
  if (copy.sameName) return <p>{t("clusterBackups.copy.sameName", copy.source)}</p>
  return (
    <div className="flex flex-col gap-3">
      <p>{t("clusterBackups.copy.into", copy.source, copy.into)}</p>
      <p>{t("clusterBackups.copy.keep", copy.source)}</p>
      {copy.hosts.length > 0 ? (
        <p>{t("clusterBackups.copy.hosts", t("common.list", copy.hosts), copy.source)}</p>
      ) : null}
      {copy.references.length > 0 ? (
        <div className="flex flex-col gap-1">
          <p>{t("clusterBackups.copy.references", copy.source, copy.references.length)}</p>
          <ul className="max-h-48 overflow-y-auto rounded-lg border border-main type-meta">
            {copy.references.map((r) => (
              <li
                key={`${r.kind}/${r.name}/${r.container ?? ""}/${r.key}`}
                className="truncate border-t border-rule px-3 py-1.5 first:border-t-0"
                translate="no"
              >
                {`${r.kind}/${r.name}${t("common.sep")}${r.key}${r.value ? `${t("common.colon")}${r.value}` : ""}`}
              </li>
            ))}
          </ul>
          <p className="type-meta text-muted-foreground">{t("clusterBackups.copy.rewriteLater")}</p>
        </div>
      ) : null}
      {copy.secrets.length > 0 ? (
        <p className="type-meta text-muted-foreground">
          {t("clusterBackups.copy.secrets", t("common.list", copy.secrets))}
        </p>
      ) : null}
      {copy.unchecked.length > 0 ? (
        <p className="type-meta text-muted-foreground">
          {t("clusterBackups.copy.unchecked", t("common.list", copy.unchecked))}
        </p>
      ) : null}
      {copy.room ? <Room room={copy.room} /> : null}
      <p className="type-meta text-muted-foreground">
        {t("clusterBackups.copy.webhooks", copy.source)}
      </p>
    </div>
  )
}

/** What a domain move does, under the writes. */
export function MoveFacts({
  plan,
  from,
  to,
}: {
  plan: Schemas["BackupsPlan"]
  from: string
  to: string
}) {
  const t = useT()
  const hosts = plan.hosts ?? []
  const managed = plan.managed ?? []
  return (
    <div className="flex flex-col gap-2">
      {hosts.length > 0 ? (
        <p>{t("clusterBackups.move.hosts", t("common.list", hosts), from, to)}</p>
      ) : null}
      {managed.length > 0 ? (
        <p className="type-meta text-muted-foreground">
          {t("clusterBackups.move.managed", t("common.list", managed))}
        </p>
      ) : null}
      <p className="type-meta text-muted-foreground">{t("clusterBackups.move.brief")}</p>
    </div>
  )
}

/** Rewrites the addresses of the original in the copy: a plan of the copy, which reads its
 * Secrets' values only now, because the person asked. */
function Rewrite({ source, into }: { source: string; into: string }) {
  const t = useT()
  const planner = usePlanner(into)
  const gate = useGate(into, "changeServices")
  const targets = useTargets(into)
  const configMaps = useQuery(configMapsQuery(into)).data
  const secrets = useQuery(objectsQuery(into, { kinds: ["core/secrets"] })).data
  const [problem, setProblem] = useState<unknown>(undefined)
  const [busy, setBusy] = useState(false)
  const plan = async () => {
    setBusy(true)
    setProblem(undefined)
    try {
      const names = ownSecrets(
        (secrets?.items ?? []).filter((k) => k.resource === "secrets").flatMap((k) => k.items),
      )
      const revealed = await Promise.all(
        names.map(async (name) => ({ name, data: await revealSecret(into, name) })),
      )
      const changes = rewriteChanges(
        source,
        into,
        targets,
        (configMaps?.items ?? []).map((c) => ({ name: c.origin.name, data: c.data })),
        revealed,
      )
      if (changes.length === 0) {
        setProblem(t("clusterBackups.rewrite.none", source))
        return
      }
      planner.plan({
        title: t("clusterBackups.rewrite.title", into),
        changes,
        notes: [t("clusterBackups.rewrite.note", source, into)],
      })
    } catch (failure) {
      setProblem(failure)
    } finally {
      setBusy(false)
    }
  }
  return (
    <>
      <GateTip gate={gate}>
        <Button size="xs" variant="outline" disabled={busy} onClick={() => void plan()}>
          {t("clusterBackups.rewrite.action")}
        </Button>
      </GateTip>
      {typeof problem === "string" ? (
        <p className="basis-full type-meta text-muted-foreground">{problem}</p>
      ) : problem ? (
        <Problem error={problem} />
      ) : null}
      {planner.dialog}
    </>
  )
}

function CopyActions({
  restore,
  gate,
  onPlan,
}: {
  restore: Schemas["VeleroRestore"]
  gate: Gate
  onPlan: OnPlan
}) {
  const t = useT()
  const source = restore.namespaces[0]
  const into = restore.into
  if (!into || !source) return null
  return (
    <div className="mt-1 flex flex-col gap-1.5">
      <div className="flex flex-wrap items-center gap-2">
        <Button
          size="xs"
          variant="outline"
          render={<Link to="/projects/$project" params={{ project: into }} />}
        >
          {t("clusterBackups.openCopy")}
        </Button>
        <GateTip gate={gate}>
          <Button
            size="xs"
            variant="outline"
            onClick={() =>
              onPlan(
                { op: "moveDomains", from: source, to: into },
                t("clusterBackups.move.title", into),
                t("clusterBackups.move.action"),
              )
            }
          >
            {t("clusterBackups.move.action")}
          </Button>
        </GateTip>
        <Rewrite source={source} into={into} />
      </div>
      <p className="type-meta text-muted-foreground">{t("clusterBackups.copy.keep", source)}</p>
    </div>
  )
}

export function RestoreList({
  restores,
  gate,
  onPlan,
}: {
  restores: Schemas["VeleroRestore"][]
  gate: Gate
  onPlan: OnPlan
}) {
  const t = useT()
  if (restores.length === 0) return null
  return (
    <Section id="restore-list" title={t("clusterBackups.restores")} count={restores.length}>
      <ul className="flex flex-col">
        {restores.map((r) => (
          <li key={r.name} className="flex flex-col gap-0.5 border-t border-rule py-2.5">
            <div className="flex min-w-0 items-center gap-2.5">
              <span className="min-w-0 flex-1 truncate type-mono" translate="no">
                {r.name}
              </span>
              <StatusPill health={outcomeHealth[r.outcome]} size="sm">
                {t(`backups.${r.outcome}`)}
              </StatusPill>
              {r.started ? (
                <Ago
                  iso={r.started}
                  className="shrink-0 text-right type-meta text-muted-foreground sm:w-24"
                />
              ) : (
                <span className="shrink-0 sm:w-24" />
              )}
            </div>
            <span className="type-meta text-muted-foreground">
              {[
                t("clusterBackups.restoredFrom", r.backup),
                r.into
                  ? t("clusterBackups.restoredInto", r.namespaces[0] ?? "", r.into)
                  : t("common.list", r.namespaces),
                r.errors > 0 ? t("clusterBackups.errors", r.errors) : "",
                r.warnings > 0 ? t("clusterBackups.warnings", r.warnings) : "",
              ]
                .filter(Boolean)
                .join(t("common.sep"))}
            </span>
            {r.reason ? (
              <p className="type-meta text-fail-strong" translate="no">
                {r.reason}
              </p>
            ) : null}
            {r.outcome === "succeeded" || r.outcome === "partiallyFailed" ? (
              <CopyActions restore={r} gate={gate} onPlan={onPlan} />
            ) : null}
          </li>
        ))}
      </ul>
    </Section>
  )
}
