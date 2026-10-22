import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { useState } from "react"
import PlusIcon from "~icons/ph/plus"
import WarningIcon from "~icons/ph/warning"

import type { Schemas } from "@/api/client"
import { databasesQuery, nodesQuery } from "@/api/queries"
import { Ago } from "@/components/bits"
import { FormField, quantity } from "@/components/form-field"
import { ConnectFields, newChoice, pickedOf, useTargets, type Choice } from "@/components/connect"
import { PlanDrawer } from "@/components/plan/plan-drawer"
import { usePlanStep } from "@/components/plan/planner"
import { StatusPill } from "@/components/status"
import { useT } from "@/i18n"
import { databaseLink } from "@/lib/place"
import { connectChanges, pendingDatabase, variableProblem } from "@/model/connect"
import {
  createDatabase,
  engineName,
  engines,
  lastRun,
  memberChoices,
  nameProblem,
  allowedVersion,
  blockedVersions,
  newDatabase,
  presetResources,
  sizePresets,
  stateHealth,
  versions,
  type Database,
  type Engine,
  type NewDatabase,
} from "@/model/databases"
import { Alert, AlertDescription } from "@zeabur/design-system/components/alert"
import { Button } from "@zeabur/design-system/components/button"
import { Input } from "@zeabur/design-system/components/input"
import { Select } from "@zeabur/design-system/components/select"

export const useDatabases = (project: string) => useQuery(databasesQuery(project))

/** Engines whose operator is installed and readable. */
export const installedEngines = (data: Schemas["ProjectDatabases"]) =>
  data.operators.filter((o) => o.state === "installed").map((o) => o.engine)

export function DatabaseState({
  database,
  size = "sm",
}: {
  database: Database
  size?: "sm" | "md"
}) {
  const t = useT()
  return (
    <StatusPill health={stateHealth[database.state]} size={size}>
      {database.state === "ready" || database.members <= 1
        ? t(`db.state.${database.state}`)
        : `${t(`db.state.${database.state}`)}${t("common.sep")}${t("db.membersReady", database.ready, database.members)}`}
    </StatusPill>
  )
}

export function DatabaseRow({ project, database }: { project: string; database: Database }) {
  const t = useT()
  const run = lastRun(database)
  return (
    <li className="flex items-center gap-2.5 border-t border-rule py-2.75">
      <div className="flex min-w-0 flex-1 flex-col">
        <Link
          {...databaseLink(project, database)}
          className="truncate type-mono hover:underline"
          translate="no"
        >
          {database.name}
        </Link>
        <span className="truncate type-meta text-muted-foreground">
          {engineName(database.engine)}
          {database.version ? ` ${database.version}` : ""}
          {t("common.sep")}
          {run ? (
            <>
              {t("db.backup.last")} <Ago iso={run.finished ?? run.started} />
            </>
          ) : database.backups.schedules.length > 0 ? (
            t("db.backup.noRun")
          ) : (
            t("db.backup.none")
          )}
        </span>
      </div>
      <DatabaseState database={database} />
    </li>
  )
}

/** What keeps databases from being listed or created: no operator, or no right to read one. */
export function OperatorNotes({ data }: { data: Schemas["ProjectDatabases"] }) {
  const t = useT()
  const installed = data.operators.filter((o) => o.state === "installed")
  const refused = data.operators.filter((o) => o.state === "refused")
  return (
    <>
      {installed.length === 0 && refused.length === 0 ? (
        <p className="border-t border-rule py-2.5 type-meta text-muted-foreground">
          {t("db.noOperator")}{" "}
          <Link to="/cluster/components" className="text-foreground hover:underline">
            {t("backups.seeComponents")}
          </Link>
        </p>
      ) : null}
      {refused.map((o) => (
        <p key={o.engine} className="border-t border-rule py-2.5 type-meta text-muted-foreground">
          {t("db.refused", engineName(o.engine))}
        </p>
      ))}
    </>
  )
}

/** "New database", and its form, when an operator is installed. */
export function NewDatabaseButton({
  project,
  data,
  size = "xs",
}: {
  project: string
  data: Schemas["ProjectDatabases"]
  size?: "xs" | "sm"
}) {
  const t = useT()
  const [open, setOpen] = useState(false)
  const available = installedEngines(data)
  if (available.length === 0) return null
  return (
    <>
      <Button variant="outline" size={size} onClick={() => setOpen(true)}>
        <PlusIcon aria-hidden="true" data-icon="inline-start" />
        {t("db.new")}
      </Button>
      {open ? (
        <NewDatabaseDrawer
          project={project}
          engines={available}
          taken={data.items.map((d) => d.name)}
          onClose={() => setOpen(false)}
        />
      ) : null}
    </>
  )
}

export function NewDatabaseDrawer({
  project,
  engines: available,
  engine,
  taken,
  onClose,
}: {
  project: string
  engines: Engine[]
  /** The engine to start with, when offered. */
  engine?: Engine
  taken: string[]
  onClose: () => void
}) {
  const t = useT()
  const nodeList = useQuery(nodesQuery).data?.items ?? []
  const nodes = nodeList.length || 1
  const kernels = nodeList.map((n) => n.kernelVersion)
  const first =
    engine && available.includes(engine)
      ? engine
      : (engines.find((e) => available.includes(e)) ?? "postgresql")
  const [entered, setForm] = useState<NewDatabase>(() =>
    newDatabase(first, nodes, blockedVersions(first, kernels)),
  )
  const [membersPicked, setMembersPicked] = useState(false)
  // The nodes can load after the form opened, with a kernel that refuses its version.
  const blocked = blockedVersions(entered.engine, kernels)
  const form = {
    ...entered,
    version: allowedVersion(entered.engine, entered.version, blocked),
    members: membersPicked ? entered.members : newDatabase(entered.engine, nodes).members,
  }
  const [tried, setTried] = useState(false)
  const step = usePlanStep(project, onClose)
  const targets = useTargets(project)
  const [choice, setChoice] = useState<Choice>(() => newChoice(form))
  const set = (patch: Partial<NewDatabase>) => setForm((f) => ({ ...f, ...patch }))
  const nameIssue = nameProblem(form.name, taken)
  const storageBad = !quantity.test(form.storage.trim())
  const resources = presetResources[form.engine][form.size]
  const pending = pendingDatabase(form, project)
  const picked = pickedOf(targets, choice)
  const review = () => {
    setTried(true)
    if (nameIssue || storageBad || (picked.length > 0 && variableProblem(choice.name))) return
    const names = t(
      "common.list",
      picked.map((s) => s.name),
    )
    step.plan({
      title: t("db.planCreate", form.name),
      changes: [
        ...createDatabase({ ...form, storage: form.storage.trim() }),
        ...connectChanges(pending, picked, choice),
      ],
      notes: picked.length > 0 ? [t("connect.waitsReady", names, form.name)] : undefined,
    })
  }
  return (
    <PlanDrawer
      step={step}
      onClose={onClose}
      title={t("db.new")}
      description={t("db.newBody")}
      footer={
        <div className="flex justify-end gap-2">
          <Button variant="ghost" size="sm" onClick={onClose}>
            {t("edit.cancel")}
          </Button>
          <Button size="sm" onClick={review}>
            {t("edit.review")}
          </Button>
        </div>
      }
    >
      <form
        className="flex flex-col gap-4"
        onSubmit={(event) => {
          event.preventDefault()
          review()
        }}
      >
        {available.length > 1 ? (
          <FormField label={t("db.field.engine")}>
            <Select
              size="sm"
              value={form.engine}
              onValueChange={(engine) => {
                setForm({
                  ...newDatabase(
                    engine as Engine,
                    nodes,
                    blockedVersions(engine as Engine, kernels),
                  ),
                  name: form.name,
                })
                setMembersPicked(false)
                setChoice((c) => newChoice({ engine: engine as Engine }, c.picked))
              }}
              items={available.map((e) => ({ value: e, label: t(`db.engine.${e}`) }))}
            />
          </FormField>
        ) : null}
        <FormField
          label={t("db.field.name")}
          errors={tried && nameIssue ? [t(`db.name.${nameIssue}`)] : []}
        >
          <Input
            size="sm"
            mono
            value={form.name}
            autoComplete="off"
            spellCheck={false}
            onChange={(event) => set({ name: event.target.value.trim() })}
          />
        </FormField>
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
          <FormField
            label={t("db.field.version")}
            hint={
              blocked.length > 0
                ? t("db.kernelBlocked", `${engineName(form.engine)} ${blocked.join(", ")}`)
                : undefined
            }
          >
            <Select
              size="sm"
              value={form.version}
              onValueChange={(version) => set({ version })}
              translate="no"
              items={versions[form.engine]
                .filter((v) => !blocked.includes(v.label))
                .map((v) => ({
                  value: v.label,
                  label: `${engineName(form.engine)} ${v.label}`,
                }))}
            />
          </FormField>
          <FormField
            label={t("db.field.members")}
            hint={
              form.engine === "mongodb" && nodes > 1 && form.members < 3
                ? t("db.replicaSetHint")
                : nodes <= 1 && form.members === 1
                  ? t("db.onOneNode")
                  : undefined
            }
          >
            <Select
              size="sm"
              value={String(form.members)}
              onValueChange={(members) => {
                setMembersPicked(true)
                set({ members: Number(members) })
              }}
              items={memberChoices[form.engine].map((n) => ({
                value: String(n),
                label: String(n),
              }))}
            />
          </FormField>
          <FormField
            label={t("db.field.storage")}
            hint={t("db.storageHint")}
            errors={tried && storageBad ? [t("storage.problem.badSize")] : []}
          >
            <Input
              size="sm"
              mono
              value={form.storage}
              onChange={(event) => set({ storage: event.target.value })}
            />
          </FormField>
          <FormField
            label={t("db.field.size")}
            hint={t(
              "db.sizeDetail",
              resources.requests.cpu,
              resources.requests.memory,
              resources.limits.memory,
            )}
          >
            <Select
              size="sm"
              value={form.size}
              onValueChange={(size) => set({ size: size as NewDatabase["size"] })}
              items={sizePresets.map((p) => ({ value: p, label: t(`db.size.${p}`) }))}
            />
          </FormField>
        </div>
        {form.members === 1 ? (
          <Alert variant="warn">
            <WarningIcon aria-hidden="true" />
            <AlertDescription>{t("db.noRedundancy", false)}</AlertDescription>
          </Alert>
        ) : null}
        <ConnectFields
          database={pending}
          targets={targets}
          choice={choice}
          onChange={setChoice}
          optional
          tried={tried}
        />
      </form>
    </PlanDrawer>
  )
}
