// A Velero backup restored into a new project while the original runs, or under its own name
// once the original is gone, both without domains; then the domains brought back and, for a copy,
// the addresses moved over.
import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { useState } from "react"
import WarningIcon from "~icons/ph/warning"

import { type Schemas, toProblem } from "@/api/client"
import {
  configMapsQuery,
  deletedBackupProjectsQuery,
  objectsQuery,
  projectsQuery,
  revealSecret,
} from "@/api/queries"
import { Ago } from "@/components/bits"
import { useTargets } from "@/components/connect"
import { FormField, NameField } from "@/components/form-field"
import { GateTip, useGate } from "@/components/gate"
import { usePlanner } from "@/components/plan/planner"
import { Problem } from "@/components/problem"
import { StatusPill } from "@/components/status"
import { type Translate, useT } from "@/i18n"
import { useQuantityFormat } from "@/lib/quantity"
import { outcomeHealth } from "@/model/cluster-backups"
import { restoredName } from "@/model/database-restore"
import {
  afterRestore,
  ownSecrets,
  restoreChoices,
  restoreProblem,
  rewriteChanges,
} from "@/model/restore-copy"
import type { Gate } from "@/model/rights"
import { FormButtons, Section } from "@/routes/cluster/section"
import { Alert, AlertDescription } from "@zeabur/design-system/components/alert"
import { Button } from "@zeabur/design-system/components/button"
import { Select } from "@zeabur/design-system/components/select"

type Request = Schemas["BackupsPlanRequest"]
type OnPlan = (request: Request, title: string, action?: string) => void
type Copy = Schemas["RestoreCopy"]

const NAMESPACE_MAX = 63

/** The projects a backup can restore, with the deleted ones it holds marked as such. */
function useChoices(backup: Schemas["VeleroBackup"]) {
  const t = useT()
  const projects = useQuery(projectsQuery).data?.items ?? []
  const deleted = useQuery({
    ...deletedBackupProjectsQuery(backup.name),
    enabled: backup.namespaces.length === 0,
  })
  const choices = restoreChoices(backup.namespaces, projects, deleted.data?.items)
  const choiceLabel = (name: string) =>
    projects.some((p) => p.name === name) ? name : t("clusterBackups.deletedChoice", name)
  return { projects, deleted, choices, choiceLabel }
}

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
  const { projects, deleted, choices, choiceLabel } = useChoices(backup)
  const [namespace, setNamespace] = useState(choices.length === 1 ? (choices[0] ?? "") : "")
  const [typed, setTyped] = useState<string | undefined>(undefined)
  const [tried, setTried] = useState(false)
  const exists = projects.some((p) => p.name === namespace)
  const into = typed ?? (namespace ? restoredName(namespace, new Date(), NAMESPACE_MAX) : "")
  const problem = restoreProblem(namespace, into, projects)
  return (
    <form
      className="mt-2 flex flex-col gap-4 rounded-xl border border-main bg-card p-5"
      aria-busy={deleted.fetchStatus === "fetching"}
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
            ...choices.map((name) => ({ value: name, label: choiceLabel(name) })),
          ]}
        />
      </FormField>
      <DeletedNote fetching={deleted.fetchStatus === "fetching"} error={deleted.error} />
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

/** While the backup is read for deleted projects, or why it could not be. */
function DeletedNote({ fetching, error }: { fetching: boolean; error: unknown }) {
  const t = useT()
  if (!fetching && !error) return null
  return (
    <p className="type-meta text-muted-foreground">
      {fetching
        ? t("clusterBackups.readingDeleted")
        : t("clusterBackups.deletedUnread", toProblem(error).detail)}
    </p>
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
  const { memory, cpuRequest } = useQuantityFormat()
  const need = t("clusterBackups.copy.amount", cpuRequest(room.needCpu), memory(room.needMemory))
  const free = t("clusterBackups.copy.amount", cpuRequest(room.freeCpu), memory(room.freeMemory))
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

/** Names in a bounded, wrapping list. */
function Names({ names }: { names: string[] }) {
  return (
    <ul
      className="flex max-h-32 flex-wrap gap-x-3 gap-y-1 overflow-y-auto type-meta type-mono"
      translate="no"
    >
      {names.map((name) => (
        <li key={name}>{name}</li>
      ))}
    </ul>
  )
}

function Listed({ text, names }: { text: string; names: string[] }) {
  if (names.length === 0) return null
  return (
    <div className="flex flex-col gap-1">
      <p>{text}</p>
      <Names names={names} />
    </div>
  )
}

/** What a restore plan does to the projects, under the writes; the lists come from the backup. */
export function CopyFacts({ copy }: { copy: Copy }) {
  const t = useT()
  const inventory = copy.inventory
  const routes = inventory?.routes ?? []
  const services = inventory?.services ?? []
  return (
    <div className="flex flex-col gap-3">
      {copy.sameName ? (
        <p>{t("clusterBackups.copy.sameName", copy.source)}</p>
      ) : (
        <>
          <p>{t("clusterBackups.copy.into", copy.source, copy.into)}</p>
          <p>{t("clusterBackups.copy.keep", copy.source)}</p>
        </>
      )}
      {inventory && routes.length === 0 ? null : (
        <p>
          {routes.length > 0
            ? t("clusterBackups.copy.domainsLast", t("common.list", routes))
            : t("clusterBackups.copy.domainsLastAny")}
        </p>
      )}
      {copy.sameName || !inventory ? null : (
        <>
          <Listed
            text={t("clusterBackups.copy.cronJobs", inventory.cronJobs.length)}
            names={inventory.cronJobs}
          />
          <Listed
            text={t("clusterBackups.copy.services", copy.source, copy.into, services[0] ?? "")}
            names={services}
          />
          <Listed text={t("clusterBackups.copy.secrets", copy.source)} names={inventory.secrets} />
        </>
      )}
      {copy.unread ? (
        <p className="type-meta text-muted-foreground">
          {t("clusterBackups.copy.unread", copy.unread)}
        </p>
      ) : null}
      {copy.room ? <Room room={copy.room} /> : null}
      {copy.sameName ? null : (
        <p className="type-meta text-muted-foreground">
          {t("clusterBackups.copy.webhooks", copy.source)}
        </p>
      )}
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

/** What bringing domains back from a backup does, or which project still serves them and the
 * move that frees them. */
export function AttachFacts({
  plan,
  to,
  onMove,
}: {
  plan: Schemas["BackupsPlan"]
  to: string
  onMove: (from: string) => void
}) {
  const t = useT()
  const held = plan.held ?? []
  const hosts = plan.hosts ?? []
  if (held.length === 0)
    return (
      <div className="flex flex-col gap-2">
        <p>{t("clusterBackups.attach.hosts", t("common.list", hosts), to)}</p>
        <p className="type-meta text-muted-foreground">{t("clusterBackups.attach.brief")}</p>
      </div>
    )
  const projects = [...new Set(held.map((h) => h.project))]
  return (
    <div className="flex flex-col gap-3">
      {projects.map((project) => (
        <Alert key={project} variant="warn" role="note">
          <WarningIcon aria-hidden="true" />
          <AlertDescription>
            <div className="flex flex-col items-start gap-2">
              <span>
                {t(
                  "clusterBackups.attach.held",
                  t(
                    "common.list",
                    held.filter((h) => h.project === project).map((h) => h.host),
                  ),
                  project,
                  to,
                )}
              </span>
              <Button size="xs" variant="outline" onClick={() => onMove(project)}>
                {t("clusterBackups.attach.move", project)}
              </Button>
            </div>
          </AlertDescription>
        </Alert>
      ))}
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
  const next = afterRestore(restore)
  if (!next) return null
  return (
    <div className="mt-1 flex flex-col gap-1.5">
      <div className="flex flex-wrap items-center gap-2">
        <Button
          size="xs"
          variant="outline"
          render={<Link to="/projects/$project" params={{ project: next.project }} />}
        >
          {t("clusterBackups.openCopy")}
        </Button>
        <GateTip gate={gate}>
          <Button
            size="xs"
            variant="outline"
            data-testid="attach-domains"
            onClick={() =>
              onPlan(
                next.attach,
                t("clusterBackups.attach.title", next.project),
                t("clusterBackups.attach.action"),
              )
            }
          >
            {t("clusterBackups.attach.action")}
          </Button>
        </GateTip>
        {next.rewrite ? <Rewrite source={next.rewrite.source} into={next.rewrite.into} /> : null}
      </div>
      {next.rewrite ? (
        <p className="type-meta text-muted-foreground">
          {t("clusterBackups.copy.keep", next.rewrite.source)}
        </p>
      ) : null}
    </div>
  )
}

function restoredWhat(t: Translate, r: Schemas["VeleroRestore"]): string {
  if ((r.resources?.length ?? 0) > 0)
    return t("clusterBackups.domainsBack", r.into ?? r.namespaces[0] ?? "")
  if (r.into) return t("clusterBackups.restoredInto", r.namespaces[0] ?? "", r.into)
  return t("common.list", r.namespaces)
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
                restoredWhat(t, r),
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
