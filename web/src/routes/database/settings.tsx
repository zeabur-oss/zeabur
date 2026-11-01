import { useState, type ReactNode } from "react"

import { PartNav } from "@/components/part-nav"
import { CopyText } from "@/components/copy-text"
import { AmountInput, SizeDetail } from "@/components/amount-input"
import { FormField } from "@/components/form-field"
import { ObjectYaml } from "@/components/store-detail"
import { GateNote, GateTip, useGate } from "@/components/gate"
import { kindWord, useGrowth } from "@/components/storage-class"
import { usePlanner } from "@/components/plan/planner"
import { useT } from "@/i18n"
import { plainAmount } from "@/lib/quantity"
import {
  commands,
  databaseAddress,
  memberChoices,
  presetOf,
  presetResources,
  engineName,
  reshapeDatabase,
  storageFixed,
  sizePresets,
  storageProblem,
  type Database,
  type SizePreset,
} from "@/model/databases"
import type { ServiceMode } from "@/routes/search"
import { ModeSwitch } from "@/routes/service/views"
import { Button } from "@zeabur-oss/design-system/components/button"
import { SectionHeader } from "@zeabur-oss/design-system/components/section-header"
import { Select } from "@zeabur-oss/design-system/components/select"

const parts = ["compute", "storage"] as const

/** Instances, size and storage; or, in its YAML view, the operator's object and kubectl. */
export function DatabaseSettings({
  project,
  database,
  mode,
  onMode,
}: {
  project: string
  database: Database
  mode?: ServiceMode
  onMode: (mode?: ServiceMode) => void
}) {
  const t = useT()
  return (
    <div className="flex flex-col gap-6">
      <ModeSwitch tab="settings" mode={mode} onMode={onMode}>
        {mode ? null : (
          <PartNav label={t("db.settings")} parts={parts} name={(part) => t(`part.${part}`)} />
        )}
      </ModeSwitch>
      {mode === "yaml" ? (
        <div className="flex flex-col gap-8">
          <DatabaseYaml project={project} database={database} />
          <Commands project={project} database={database} />
        </div>
      ) : (
        <Shape key={shapeKey(database)} project={project} database={database} />
      )}
    </div>
  )
}

const shapeKey = (d: Database) => `${d.members}/${d.storage}/${JSON.stringify(d.resources)}`

function Shape({ project, database }: { project: string; database: Database }) {
  const t = useT()
  const gate = useGate(project, "changeDatabases")
  const planner = usePlanner(project)
  const preset = presetOf(database.engine, database.resources)
  const [members, setMembers] = useState(database.members)
  const [size, setSize] = useState<SizePreset | "custom">(preset ?? "custom")
  const [storage, setStorage] = useState(database.storage ?? "")
  const [tried, setTried] = useState(false)
  const problem = storageProblem(database.storage, storage)
  const changed =
    members !== database.members ||
    size !== (preset ?? "custom") ||
    storage.trim() !== (database.storage ?? "")
  const choices = memberChoices[database.engine].includes(database.members)
    ? memberChoices[database.engine]
    : [...memberChoices[database.engine], database.members].sort((a, b) => a - b)
  const disabled = !gate.open
  const { storageClass, fixed, description } = useDatabaseGrowth(project, database)
  const review = () => {
    setTried(true)
    if (problem) return
    planner.plan({
      title: t("db.reshape", database.name),
      changes: reshapeDatabase(database, {
        members,
        size: size === "custom" ? undefined : size,
        storage,
      }),
    })
  }
  const footer = (
    <div className="flex justify-end">
      <GateTip gate={gate}>
        <Button size="sm" type="submit" disabled={!changed}>
          {t("edit.review")}
        </Button>
      </GateTip>
    </div>
  )
  return (
    <form
      className="flex min-w-0 flex-col gap-8"
      onSubmit={(event) => {
        event.preventDefault()
        review()
      }}
    >
      <GateNote gate={gate} />
      <Part id="compute" title={t("part.compute")} description={t("db.computeHint")}>
        <div className="grid grid-cols-1 items-start gap-4 sm:grid-cols-2">
          <FormField label={t("db.field.members")}>
            <Select
              size="sm"
              value={String(members)}
              disabled={disabled}
              onValueChange={(value) => setMembers(Number(value))}
              items={choices.map((n) => ({ value: String(n), label: String(n) }))}
            />
          </FormField>
          <FormField
            label={t("db.field.size")}
            hint={
              size === "custom" ? (
                t("db.customSize")
              ) : (
                <SizeDetail resources={presetResources[database.engine][size]} />
              )
            }
          >
            <Select
              size="sm"
              value={size}
              disabled={disabled}
              onValueChange={setSize}
              items={[
                ...(preset ? [] : [{ value: "custom" as const, label: t("db.fact.custom") }]),
                ...sizePresets.map((p) => ({ value: p, label: t(`db.size.${p}`) })),
              ]}
            />
          </FormField>
        </div>
      </Part>
      <Part id="storage" title={t("part.storage")} description={description}>
        <div className="grid grid-cols-1 items-start gap-4 sm:grid-cols-2">
          <FormField
            label={t("db.field.storage")}
            hint={
              storageClass
                ? t("storage.option", kindWord(t, storageClass), storageClass.name, false)
                : undefined
            }
            errors={
              tried && problem
                ? [
                    problem === "shrink"
                      ? t("db.storage.shrink", plainAmount(t, database.storage ?? "", "bytes"))
                      : t("db.storage.invalid"),
                  ]
                : []
            }
          >
            <AmountInput
              dimension="bytes"
              value={storage}
              disabled={disabled || fixed}
              onChange={setStorage}
            />
          </FormField>
        </div>
      </Part>
      {footer}
      {planner.dialog}
    </form>
  )
}

/** Whether the volume can grow, and why not. */
function useDatabaseGrowth(project: string, database: Database) {
  const t = useT()
  const growth = useGrowth(project, database.storageClass)
  if (!storageFixed(database.engine)) return growth
  return {
    storageClass: growth.storageClass,
    fixed: true,
    description: t("db.storage.operatorFixed", engineName(database.engine)),
  }
}

function Part({
  id,
  title,
  description,
  children,
}: {
  id: (typeof parts)[number]
  title: string
  description: string
  children: ReactNode
}) {
  return (
    <section id={id} aria-labelledby={`${id}-title`} className="flex scroll-mt-6 flex-col gap-3">
      <SectionHeader id={`${id}-title`} title={title} description={description} />
      <div className="flex flex-col gap-4 rounded-xl border border-v2-border bg-card p-5">
        {children}
      </div>
    </section>
  )
}

/** The operator's object as YAML, as a service's workload shows in its YAML view. */
function DatabaseYaml({ project, database }: { project: string; database: Database }) {
  return <ObjectYaml project={project} address={databaseAddress(database)} />
}

function Commands({ project, database }: { project: string; database: Database }) {
  const t = useT()
  const { describe, shell } = commands(project, database)
  const rows = [
    { label: t("db.command.describe"), command: describe },
    { label: t("db.command.shell"), command: shell },
  ]
  return (
    <section aria-labelledby="db-commands" className="flex flex-col gap-3">
      <SectionHeader id="db-commands" title={t("db.commands")} />
      <ul className="flex flex-col gap-3">
        {rows.map((row) => (
          <li key={row.label} className="flex flex-col gap-1.5">
            <span className="type-meta text-muted-foreground">{row.label}</span>
            <div className="flex flex-wrap items-center gap-x-2 gap-y-1 rounded-lg border bg-muted/40 py-1.5 pr-1.5 pl-3">
              <code className="min-w-48 flex-1 type-mono break-all" translate="no">
                {row.command}
              </code>
              <div className="ml-auto flex items-center">
                <CopyText value={() => row.command} label={t("common.copy")} />
              </div>
            </div>
          </li>
        ))}
      </ul>
    </section>
  )
}
