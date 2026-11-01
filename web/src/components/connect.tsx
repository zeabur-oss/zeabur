import { useQuery } from "@tanstack/react-query"
import { useId, useState } from "react"
import PlugsConnectedIcon from "~icons/ph/plugs-connected"

import { databasesQuery, namespaceQuery, projectQuery } from "@/api/queries"
import { FormField } from "@/components/form-field"
import { GateTip, useGate } from "@/components/gate"
import { PlanDrawer } from "@/components/plan/plan-drawer"
import { usePlanStep, type PlanAsk } from "@/components/plan/planner"
import { useT, type Translate } from "@/i18n"
import {
  candidates,
  connectChanges,
  connected,
  nameProblem,
  namesOf,
  pgExtras,
  suggestedName,
  variablesFor,
  type Connectable,
  type NameProblem,
} from "@/model/connect"
import type { Service } from "@/model/service"
import { Button } from "@zeabur-oss/design-system/components/button"
import { Checkbox } from "@zeabur-oss/design-system/components/checkbox"
import { Input } from "@zeabur-oss/design-system/components/input"
import { Label } from "@zeabur-oss/design-system/components/label"
import { Select } from "@zeabur-oss/design-system/components/select"

export interface Choice {
  /** `kind/name` of each service picked. */
  picked: string[]
  /** As typed; the suggested name until then. */
  name?: string
  extras: boolean
  /** The S3 bucket; the store's first until chosen. */
  bucket?: string
}

const keyOf = (s: { kind: string; name: string }) => `${s.kind}/${s.name}`

const bucketOf = (database: Connectable, choice: Choice) => choice.bucket ?? database.buckets?.[0]

const newChoice = (picked: string[] = []): Choice => ({ picked, extras: false })

/** The name the connection uses in these services, and what is wrong with it. */
function namingOf(database: Connectable, targets: Service[], choice: Choice) {
  const fresh = pickedOf(targets, choice).filter((s) => !connected(s, database))
  const name = choice.name ?? suggestedName(database, fresh, choice.extras)
  return { fresh, name, problem: nameProblem(database, fresh, name, choice.extras) }
}

const nameProblemText = (t: Translate, problem: NameProblem) =>
  problem.kind === "invalid"
    ? t("connect.badName")
    : t("connect.taken", problem.name, problem.service)

/** Services the person may change, for connecting a database. */
export function useTargets(project: string): Service[] {
  const services = useQuery(projectQuery(project)).data?.services ?? []
  const rights = useQuery(namespaceQuery(project)).data?.workloadPatch ?? []
  return candidates(services, rights)
}

const pickedOf = (targets: Service[], choice: Choice) =>
  targets.filter((s) => choice.picked.includes(keyOf(s)))

/** The plan of connecting; `pending` when the database is created in the same plan. */
function connectAsk(
  t: Translate,
  database: Connectable,
  targets: Service[],
  choice: Choice,
): PlanAsk {
  const names = t(
    "common.list",
    targets.map((s) => s.name),
  )
  return {
    title: t("connect.planTitle", database.name, names),
    changes: connectChanges(database, targets, {
      ...choice,
      name: choice.name ?? suggestedName(database, targets, choice.extras),
      bucket: bucketOf(database, choice),
    }),
  }
}

/** The services to connect, and the variables each gets. */
function connectNote(t: Translate, database: Connectable, name: string, extras: boolean) {
  const secret = database.app?.secret ?? ""
  if (database.engine === "postgresql") return t("connect.note.postgresql", secret)
  if (database.engine === "s3") return t("connect.note.s3", secret)
  return t("connect.note.url", secret, name, namesOf(database, name, extras).password ?? "")
}

/** The engine's own option: the S3 bucket, or PostgreSQL's extra variables. */
function EngineOption({
  database,
  choice,
  onChange,
}: {
  database: Connectable
  choice: Choice
  onChange: (choice: Choice) => void
}) {
  const t = useT()
  const id = useId()
  if (database.engine === "s3" && database.buckets?.length)
    return (
      <FormField label={t("connect.bucket")}>
        <Select
          size="sm"
          value={bucketOf(database, choice) ?? ""}
          translate="no"
          onValueChange={(bucket) => onChange({ ...choice, bucket })}
          items={database.buckets.map((b) => ({ value: b, label: b }))}
        />
      </FormField>
    )
  if (database.engine !== "postgresql") return null
  return (
    <div className="flex items-start gap-2.5">
      <Checkbox
        id={`${id}-extras`}
        checked={choice.extras}
        onCheckedChange={(extras) => onChange({ ...choice, extras })}
        className="mt-0.5"
      />
      <Label htmlFor={`${id}-extras`}>{t("connect.extras", t("common.list", pgExtras))}</Label>
    </div>
  )
}

export function ConnectFields({
  database,
  targets,
  choice,
  onChange,
  fixed = false,
  optional = false,
  tried = false,
}: {
  database: Connectable
  targets: Service[]
  choice: Choice
  onChange: (choice: Choice) => void
  /** The caller chose the service: no list. */
  fixed?: boolean
  /** Picking none is fine (a new database). */
  optional?: boolean
  tried?: boolean
}) {
  const t = useT()
  const id = useId()
  const { fresh: picked, name, problem } = namingOf(database, targets, choice)
  const full = { ...choice, name, bucket: bucketOf(database, choice) }
  const toggle = (key: string, on: boolean) =>
    onChange({
      ...choice,
      picked: on ? [...choice.picked, key] : choice.picked.filter((k) => k !== key),
    })
  return (
    <div className="flex flex-col gap-4">
      {fixed ? null : (
        <div className="flex flex-col gap-1.5">
          <span className="w-fit type-body font-medium text-main" id={`${id}-services`}>
            {optional ? t("connect.action") : t("connect.services")}
          </span>
          {optional ? (
            <span className="type-body text-secondary">{t("connect.servicesHint")}</span>
          ) : null}
          {targets.length === 0 ? (
            <p className="type-meta text-muted-foreground">{t("connect.noTargets")}</p>
          ) : (
            <ul
              aria-labelledby={`${id}-services`}
              className="max-h-56 divide-y divide-main overflow-y-auto rounded-lg border border-main"
            >
              {targets.map((s) => {
                const key = keyOf(s)
                const already = connected(s, database)
                return (
                  <li key={key} className="flex h-10 min-w-0 items-center gap-2.5 px-3">
                    <Checkbox
                      id={`${id}-${key}`}
                      checked={already || choice.picked.includes(key)}
                      disabled={already}
                      onCheckedChange={(on) => toggle(key, on)}
                    />
                    <Label htmlFor={`${id}-${key}`} className="min-w-0 flex-1">
                      <span className="truncate type-mono" translate="no">
                        {s.name}
                      </span>
                    </Label>
                    {already ? (
                      <span className="type-meta text-muted-foreground">
                        {t("connect.already")}
                      </span>
                    ) : null}
                  </li>
                )
              })}
            </ul>
          )}
        </div>
      )}
      {picked.length > 0 ? (
        <>
          <FormField
            label={t("connect.variable")}
            errors={
              (tried || choice.name !== undefined) && problem ? [nameProblemText(t, problem)] : []
            }
          >
            <Input
              size="sm"
              mono
              value={name}
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => onChange({ ...choice, name: event.target.value.trim() })}
            />
          </FormField>
          <EngineOption database={database} choice={choice} onChange={onChange} />
          <div className="flex flex-col gap-1.5">
            <span className="w-fit type-body font-medium text-main">{t("connect.adds")}</span>
            <ul className="flex flex-col">
              {picked.map((s) => (
                <li
                  key={keyOf(s)}
                  className="flex flex-col gap-x-4 gap-y-0.5 border-t border-rule py-2 sm:flex-row"
                >
                  {picked.length > 1 || !fixed ? (
                    <span
                      className="shrink-0 type-meta text-muted-foreground sm:w-32"
                      translate="no"
                    >
                      {s.name}
                    </span>
                  ) : null}
                  <span className="min-w-0 type-mono wrap-anywhere" translate="no">
                    {variablesFor(database, s, full)
                      .map((e) => e.name)
                      .join(", ")}
                  </span>
                </li>
              ))}
            </ul>
            <p className="type-meta text-muted-foreground">
              {connectNote(t, database, name, choice.extras)}
            </p>
          </div>
        </>
      ) : null}
    </div>
  )
}

/** "Connect to a service" from a database, or "Add a database connection" from a service. */
export function ConnectDrawer({
  project,
  database: fixedDatabase,
  service,
  onClose,
}: {
  project: string
  database?: Connectable
  service?: Service
  onClose: () => void
}) {
  const t = useT()
  const all = useQuery({ ...databasesQuery(project), enabled: !fixedDatabase }).data?.items ?? []
  const usable: Connectable[] = fixedDatabase ? [fixedDatabase] : all.filter((d) => d.app)
  const unusable = fixedDatabase ? [] : all.filter((d) => !d.app)
  const [chosen, setChosen] = useState(fixedDatabase?.name)
  const database = usable.find((d) => d.name === chosen) ?? usable[0]
  const everyone = useTargets(project)
  const targets = service ? [service] : everyone
  const [choice, setChoice] = useState<Choice>(() => newChoice(service ? [keyOf(service)] : []))
  const [tried, setTried] = useState(false)
  const step = usePlanStep(project, onClose)
  const naming = database ? namingOf(database, targets, choice) : undefined
  const fresh = naming?.fresh ?? []
  const review = () => {
    setTried(true)
    if (!database || !naming || fresh.length === 0 || naming.problem) return
    step.plan(connectAsk(t, database, fresh, { ...choice, name: naming.name }))
  }
  return (
    <PlanDrawer
      step={step}
      onClose={onClose}
      title={
        service ? t("connect.titleService", service.name) : t("connect.title", database?.name ?? "")
      }
      footer={
        <div className="flex justify-end gap-2">
          <Button variant="ghost" size="sm" onClick={onClose}>
            {t("edit.cancel")}
          </Button>
          <Button size="sm" disabled={fresh.length === 0} onClick={review}>
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
        {fixedDatabase ? null : usable.length === 0 ? (
          <p className="type-meta text-muted-foreground">{t("connect.noDatabases")}</p>
        ) : (
          <FormField label={t("connect.database")}>
            <Select
              size="sm"
              value={database?.name ?? ""}
              translate="no"
              onValueChange={(name) => {
                setChosen(name)
                const next = usable.find((d) => d.name === name)
                if (next) setChoice((c) => ({ ...c, name: undefined, extras: false }))
              }}
              items={usable.map((d) => ({ value: d.name, label: d.name }))}
            />
          </FormField>
        )}
        {unusable.map((d) => (
          <p key={d.name} className="type-meta text-muted-foreground">
            {t("connect.noApp", d.name)}
          </p>
        ))}
        {service && database && connected(service, database) ? (
          <p className="type-meta text-muted-foreground">{t("connect.readBy", database.name)}</p>
        ) : null}
        {database ? (
          <ConnectFields
            database={database}
            targets={targets}
            choice={choice}
            onChange={setChoice}
            fixed={!!service}
            tried={tried}
          />
        ) : null}
      </form>
    </PlanDrawer>
  )
}

/** Which services read the database or store, and connecting another. */
export function ConnectServices({ project, database }: { project: string; database: Connectable }) {
  const t = useT()
  const [open, setOpen] = useState(false)
  const services = useQuery(projectQuery(project)).data?.services ?? []
  const targets = useTargets(project)
  const connectGate = useGate(project, "changeServices")
  const readers = services.filter((s) => connected(s, database)).map((s) => s.name)
  if (!database.app)
    return (
      <p className="type-meta text-muted-foreground">
        {database.engine === "s3"
          ? t("connect.noKey", database.name)
          : t("connect.noApp", database.name)}
      </p>
    )
  return (
    <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
      <p className="min-w-0 flex-1 text-sm">
        {readers.length > 0 ? t("connect.readBy", t("common.list", readers)) : t("connect.none")}
      </p>
      {targets.some((s) => !connected(s, database)) ? (
        <GateTip gate={connectGate}>
          <Button variant="outline" size="xs" onClick={() => setOpen(true)}>
            <PlugsConnectedIcon aria-hidden="true" data-icon="inline-start" />
            {t("connect.action")}
          </Button>
        </GateTip>
      ) : null}
      {open ? (
        <ConnectDrawer project={project} database={database} onClose={() => setOpen(false)} />
      ) : null}
    </div>
  )
}

/** The services a create form connects to what it creates, and the plan's part for them. */
export function useNewConnections(project: string, pending: Connectable) {
  const t = useT()
  const targets = useTargets(project)
  const [choice, setChoice] = useState<Choice>(() => newChoice())
  const { fresh: picked, name, problem } = namingOf(pending, targets, choice)
  return {
    fieldProps: { database: pending, targets, choice, onChange: setChoice },
    reset: () => setChoice((c) => newChoice(c.picked)),
    problem: picked.length > 0 ? problem : undefined,
    changes: () =>
      connectChanges(pending, picked, { ...choice, name, bucket: bucketOf(pending, choice) }),
    notes: () =>
      picked.length > 0
        ? [
            t(
              "connect.waitsReady",
              t(
                "common.list",
                picked.map((s) => s.name),
              ),
              pending.name,
            ),
          ]
        : undefined,
  }
}
