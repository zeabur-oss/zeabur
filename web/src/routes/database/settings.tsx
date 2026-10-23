import { useQuery } from "@tanstack/react-query"
import { useState, type ReactNode } from "react"

import { objectQuery } from "@/api/queries"
import { CopyText } from "@/components/copy-text"
import { OpenInTerminal } from "@/components/dock/open-in-terminal"
import { FormField } from "@/components/form-field"
import { GateNote, GateTip, useGate } from "@/components/gate"
import { LoadingRows } from "@/components/page"
import { kindWord, useStorage } from "@/components/storage-class"
import { usePlanner } from "@/components/plan/planner"
import { Problem } from "@/components/problem"
import { YamlView } from "@/components/yaml-view"
import { useT } from "@/i18n"
import {
  commands,
  databaseAddress,
  memberChoices,
  presetOf,
  presetResources,
  reshapeDatabase,
  sizePresets,
  storageProblem,
  type Database,
  type SizePreset,
} from "@/model/databases"
import { classNamed } from "@/model/storage"
import type { ServiceMode } from "@/routes/search"
import { ModeSwitch } from "@/routes/service/views"
import { Button } from "@zeabur/design-system/components/button"
import { Input } from "@zeabur/design-system/components/input"
import { SectionHeader } from "@zeabur/design-system/components/section-header"
import { Select } from "@zeabur/design-system/components/select"

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
          <nav
            aria-label={t("db.settings")}
            className="flex flex-wrap items-center gap-x-4 gap-y-1"
          >
            {parts.map((part) => (
              <a
                key={part}
                href={`#${part}`}
                onClick={(event) => {
                  event.preventDefault()
                  document
                    .getElementById(part)
                    ?.scrollIntoView({ block: "start", behavior: "smooth" })
                  history.replaceState(history.state, "", `#${part}`)
                }}
                className="flex items-center gap-1.5 type-meta text-secondary outline-none hover:text-main focus-visible:underline"
              >
                {t(`part.${part}`)}
              </a>
            ))}
          </nav>
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
  const storageClass = classNamed(useStorage(project), database.storageClass)
  const growable = useStorage(project)?.classes.some((c) => c.allowExpansion) ?? false
  const fixed = storageClass?.allowExpansion === false
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
              size === "custom"
                ? t("db.customSize")
                : t(
                    "db.sizeDetail",
                    presetResources[database.engine][size].requests.cpu,
                    presetResources[database.engine][size].requests.memory,
                    presetResources[database.engine][size].limits.memory,
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
      <Part
        id="storage"
        title={t("part.storage")}
        description={
          storageClass === undefined
            ? t("storage.growUnknown")
            : fixed
              ? t("storage.fixed", storageClass.name, growable)
              : t("storage.grows")
        }
      >
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
                      ? t("db.storage.shrink", database.storage ?? "")
                      : t("db.storage.invalid"),
                  ]
                : []
            }
          >
            <Input
              size="sm"
              mono
              value={storage}
              disabled={disabled || fixed}
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => setStorage(event.target.value)}
            />
          </FormField>
        </div>
      </Part>
      {footer}
      {planner.dialog}
    </form>
  )
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
  const t = useT()
  const address = databaseAddress(database)
  const object = useQuery(objectQuery(project, address))
  if (object.isPending) return <LoadingRows label={t("common.loading")} rows={8} />
  if (object.isError) return <Problem error={object.error} />
  return (
    <YamlView
      project={project}
      address={address}
      yaml={object.data.yaml}
      managedBy={object.data.provenance.tool}
    />
  )
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
                <OpenInTerminal project={project} command={row.command} />
              </div>
            </div>
          </li>
        ))}
      </ul>
    </section>
  )
}
