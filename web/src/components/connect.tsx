import { useQuery } from "@tanstack/react-query"
import { useId, useState } from "react"

import { databasesQuery, namespaceQuery, projectQuery } from "@/api/queries"
import { FormField } from "@/components/form-field"
import { PlanDrawer } from "@/components/plan/plan-drawer"
import { usePlanStep, type PlanAsk } from "@/components/plan/planner"
import { useT, type Translate } from "@/i18n"
import {
  candidates,
  connectChanges,
  connected,
  mainVariable,
  pgExtras,
  variableProblem,
  variablesFor,
  type Connectable,
} from "@/model/connect"
import type { Database } from "@/model/databases"
import type { Service } from "@/model/service"
import { Button } from "@zeabur/design-system/components/button"
import { Checkbox } from "@zeabur/design-system/components/checkbox"
import { Input } from "@zeabur/design-system/components/input"
import { Label } from "@zeabur/design-system/components/label"
import { Select } from "@zeabur/design-system/components/select"

export interface Choice {
  /** `kind/name` of each service picked. */
  picked: string[]
  name: string
  extras: boolean
}

const keyOf = (s: { kind: string; name: string }) => `${s.kind}/${s.name}`

export const newChoice = (database: Pick<Database, "engine">, picked: string[] = []): Choice => ({
  picked,
  name: mainVariable(database),
  extras: false,
})

/** Services the person may change, for connecting a database. */
export function useTargets(project: string): Service[] {
  const services = useQuery(projectQuery(project)).data?.services ?? []
  const rights = useQuery(namespaceQuery(project)).data?.workloadPatch ?? []
  return candidates(services, rights)
}

export const pickedOf = (targets: Service[], choice: Choice) =>
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
    changes: connectChanges(database, targets, choice),
  }
}

/** The services to connect, and the variables each gets. */
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
  const picked = pickedOf(targets, choice).filter((s) => !connected(s, database))
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
            errors={tried && variableProblem(choice.name) ? [t("connect.badName")] : []}
          >
            <Input
              size="sm"
              mono
              value={choice.name}
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => onChange({ ...choice, name: event.target.value.trim() })}
            />
          </FormField>
          {database.engine === "postgresql" ? (
            <div className="flex items-start gap-2.5">
              <Checkbox
                id={`${id}-extras`}
                checked={choice.extras}
                onCheckedChange={(extras) => onChange({ ...choice, extras })}
                className="mt-0.5"
              />
              <Label htmlFor={`${id}-extras`}>
                {t("connect.extras", t("common.list", pgExtras))}
              </Label>
            </div>
          ) : null}
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
                    {variablesFor(database, s, choice)
                      .map((e) => e.name)
                      .join(", ")}
                  </span>
                </li>
              ))}
            </ul>
            <p className="type-meta text-muted-foreground">
              {database.engine === "postgresql"
                ? t("connect.note.postgresql", database.app?.secret ?? "")
                : t("connect.note.mongodb", database.app?.secret ?? "")}
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
  database?: Database
  service?: Service
  onClose: () => void
}) {
  const t = useT()
  const all = useQuery(databasesQuery(project)).data?.items ?? []
  const usable = fixedDatabase ? [fixedDatabase] : all.filter((d) => d.app)
  const unusable = fixedDatabase ? [] : all.filter((d) => !d.app)
  const [chosen, setChosen] = useState(fixedDatabase?.name)
  const database = usable.find((d) => d.name === chosen) ?? usable[0]
  const everyone = useTargets(project)
  const targets = service ? [service] : everyone
  const [choice, setChoice] = useState<Choice>(() =>
    newChoice(database ?? { engine: "postgresql" }, service ? [keyOf(service)] : []),
  )
  const [tried, setTried] = useState(false)
  const step = usePlanStep(project, onClose)
  const fresh = database ? pickedOf(targets, choice).filter((s) => !connected(s, database)) : []
  const review = () => {
    setTried(true)
    if (!database || fresh.length === 0 || variableProblem(choice.name)) return
    step.plan(connectAsk(t, database, fresh, choice))
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
                if (next) setChoice((c) => ({ ...c, name: mainVariable(next), extras: false }))
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
