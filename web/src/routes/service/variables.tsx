import { useQuery } from "@tanstack/react-query"
import { useNavigate } from "@tanstack/react-router"
import { lazy, Suspense, useRef, useState, type ClipboardEvent, type ReactNode } from "react"
import ArrowCounterClockwiseIcon from "~icons/ph/arrow-counter-clockwise"
import ArrowRightIcon from "~icons/ph/arrow-right"
import BracketsCurlyIcon from "~icons/ph/brackets-curly"
import ClockCounterClockwiseIcon from "~icons/ph/clock-counter-clockwise"
import EyeIcon from "~icons/ph/eye"
import EyeSlashIcon from "~icons/ph/eye-slash"
import KeyIcon from "~icons/ph/key"
import ListBulletsIcon from "~icons/ph/list-bullets"
import PencilSimpleIcon from "~icons/ph/pencil-simple"
import PlusIcon from "~icons/ph/plus"
import TrashIcon from "~icons/ph/trash"

import { api, toProblem, unwrap, type Problem as ProblemBody, type Schemas } from "@/api/client"
import { configMapsQuery } from "@/api/queries"
import { CopyButton, Hint, ManagedTip, Segmented } from "@/components/bits"
import { FilterField } from "@/components/filter-field"
import { KubeDetails } from "@/components/kube-details"
import { useDraft, useHold } from "@/components/plan/draft"
import { usePlanner } from "@/components/plan/planner"
import { Problem } from "@/components/problem"
import { RefusedKinds } from "@/components/refused"
import { Alert, AlertDescription } from "@zeabur/design-system/components/alert"
import { Button } from "@zeabur/design-system/components/button"
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@zeabur/design-system/components/empty"
import { Input } from "@zeabur/design-system/components/input"
import { SectionHeader } from "@zeabur/design-system/components/section-header"
import { Select } from "@zeabur/design-system/components/select"
import { Spinner } from "@zeabur/design-system/components/spinner"
import { Textarea } from "@zeabur/design-system/components/textarea"
import { Tooltip } from "@zeabur/design-system/components/tooltip"
import { useT, type Translate } from "@/i18n"
import { createdKey, entrance } from "@/lib/created"
import { cn } from "@/lib/utils"
import type { Edge } from "@/model/connections"
import {
  changesOf,
  editedCount,
  fromDotenv,
  parseDotenv,
  pastedLines,
  pasteInto,
  problemsOf,
  quote,
  rowState,
  toDotenv,
  type EnvRow,
  type RowProblem,
} from "@/model/env-edit"
import { isLongList } from "@/model/lists"
import { configObjectsOf, type Service } from "@/model/service"

const DataEditor = lazy(() =>
  import("@/components/data-editor").then((m) => ({ default: m.DataEditor })),
)

type Revealed = Record<string, Schemas["SecretValue"]>

interface SecretState {
  values?: Revealed
  pending?: boolean
  problem?: ProblemBody
}

type Source = Schemas["ValueSource"]

export function VariablesTab({
  project,
  service,
  edges,
  refused,
}: {
  project: string
  service: Service
  edges: Edge[]
  refused: Schemas["UnreadableView"][]
}) {
  const t = useT()
  // Revealed values never go into the query cache or storage.
  const [secrets, setSecrets] = useState<Record<string, SecretState>>({})
  const [raw, setRaw] = useState(false)
  const [editor, setEditor] = useState<{
    kind: "ConfigMap" | "Secret"
    name: string
    key?: string
  }>()
  const draft = useDraft()
  const planner = usePlanner(project)
  const managed = service.provenance.tool ?? undefined
  const workload = { kind: service.kind, name: service.name }
  const configMapList = useQuery(configMapsQuery(project))
  const configMaps = new Map((configMapList.data?.items ?? []).map((c) => [c.origin.name, c.data]))
  const named = new Map<string, string[]>()
  for (const edge of edges)
    for (const evidence of edge.evidence)
      if (evidence.basis === "envValue") {
        const key = `${evidence.container}/${evidence.variable}`
        named.set(key, [...(named.get(key) ?? []), evidence.service])
      }

  const containers = service.containers
  const literal = containers.flatMap((c) =>
    c.env.filter((e) => e.source.kind === "Literal").map((e) => ({ container: c.name, env: e })),
  )
  const referenced = containers.flatMap((c) =>
    c.env.filter((e) => e.source.kind !== "Literal").map((e) => ({ container: c.name, env: e })),
  )
  const imported = containers.flatMap((c) =>
    c.importedEnv.map((i) => ({ container: c.name, imported: i })),
  )
  const secretNames = configObjectsOf(service)
    .filter((o) => o.kind === "Secret")
    .map((o) => o.name)
  const multi = containers.length > 1
  const [editing, setEditing] = useState<{ rows: EnvRow[]; text?: string }>()
  const nextId = useRef(0)
  const newId = () => `new-${nextId.current++}`
  const blankRow = (container = containers[0]?.name ?? ""): EnvRow => ({
    id: newId(),
    container,
    name: "",
    value: "",
    removed: false,
  })
  const [focus, setFocus] = useState<string>()

  const reveal = async (name: string) => {
    setSecrets((previous) => ({ ...previous, [name]: { pending: true } }))
    try {
      const answer = unwrap(
        await api.POST("/api/v1/projects/{project}/secrets/{name}/reveal", {
          params: { path: { project, name } },
          cache: "no-store",
        }),
      )
      setSecrets((previous) => ({ ...previous, [name]: { values: answer.values } }))
    } catch (error) {
      setSecrets((previous) => ({ ...previous, [name]: { problem: toProblem(error) } }))
    }
  }
  const hide = (name: string) =>
    setSecrets((previous) => {
      const next = { ...previous }
      delete next[name]
      return next
    })

  const valueOf = (env: Schemas["EnvVar"]): string | undefined => {
    const source = env.source
    if (source.kind === "Literal") return env.value ?? ""
    if (source.kind === "Secret" && source.name && source.key)
      return secrets[source.name]?.values?.[source.key]?.value
    if (source.kind === "ConfigMap" && source.name && source.key)
      return configMaps.get(source.name)?.[source.key]
    return undefined
  }

  const dotenv = [
    ...literal.map(({ env }) => `${env.name}=${quote(env.value ?? "")}`),
    ...referenced.map(({ env }) => {
      const value = valueOf(env)
      return `${env.name}=${value === undefined ? (env.source.kind === "Secret" ? "••••••••" : `# ${sourceWords(t, env.source)}`) : quote(value)}`
    }),
    ...imported.flatMap(({ imported: i }) => {
      const name = i.source.name ?? ""
      const entries =
        i.source.kind === "ConfigMap"
          ? Object.entries(configMaps.get(name) ?? {})
          : Object.entries(secrets[name]?.values ?? {}).map(
              ([k, v]) => [k, v.value] as [string, string],
            )
      return entries.length
        ? entries.map(([k, v]) => `${i.prefix ?? ""}${k}=${quote(v)}`)
        : [`# ${sourceWords(t, i.source, true)}: ${t("vars.keys", i.keyCount ?? 0)}`]
    }),
  ].join("\n")

  const fromObjects = referenced.filter(
    ({ env }) => env.source.kind === "Secret" || env.source.kind === "ConfigMap",
  )
  const fromCluster = referenced.filter(
    ({ env }) => env.source.kind !== "Secret" && env.source.kind !== "ConfigMap",
  )
  const pending = [
    ...fromObjects.flatMap(({ env }) => (env.pendingRestart ? [env.pendingRestart] : [])),
    ...imported.flatMap(({ imported: i }) => (i.pendingRestart ? [i.pendingRestart] : [])),
  ]
  const navigate = useNavigate()
  const reloads =
    !service.configReload?.auto &&
    (service.kind === "Deployment" ||
      service.kind === "StatefulSet" ||
      service.kind === "DaemonSet")
  const restart = () =>
    planner.plan({
      title: t("title.restart", service.name),
      changes: [{ op: "restart", workload }],
    })
  const total =
    literal.length +
    referenced.length +
    imported.reduce((n, i) => n + (i.imported.keyCount ?? 0), 0)
  const [query, setQuery] = useState("")
  const long = isLongList(literal.length + referenced.length + imported.length)
  const needle = long ? query.trim().toLowerCase() : ""
  const matches = (...texts: (string | null | undefined)[]) =>
    !needle || texts.some((text) => text?.toLowerCase().includes(needle))
  const shownLiteral = literal.filter(({ env }) => matches(env.name, env.value))
  const shownObjects = fromObjects.filter(({ env }) => matches(env.name, env.source.name))
  const shownImported = imported.filter(({ imported: i }) => matches(i.prefix, i.source.name))
  const shownCluster = fromCluster.filter(({ env }) => matches(env.name))
  const elsewhere = [
    ...(fromObjects.length + imported.length > 0
      ? [{ id: "vars-objects", text: t("vars.alsoObjects", fromObjects.length + imported.length) }]
      : []),
    ...(fromCluster.length > 0
      ? [{ id: "vars-cluster", text: t("vars.alsoCluster", fromCluster.length) }]
      : []),
  ]

  // The editor starts from the cluster with the variables the draft already holds applied.
  const startEditing = (add: boolean) => {
    let rows: EnvRow[] = literal.map(({ container, env }) => ({
      id: `${container}/${env.name}`,
      container,
      name: env.name,
      value: env.value ?? "",
      was: { name: env.name, value: env.value ?? "" },
      removed: false,
    }))
    for (const change of draft?.changes ?? [])
      if (
        change.op === "env" &&
        change.workload.kind === workload.kind &&
        change.workload.name === workload.name
      ) {
        const container = change.container ?? containers[0]?.name ?? ""
        for (const edit of change.changes) {
          const at = rows.findIndex((r) => r.container === container && r.was?.name === edit.name)
          if (edit.op === "remove")
            rows = rows.map((r, i) => (i === at ? { ...r, removed: true } : r))
          else if (at !== -1)
            rows = rows.map((r, i) => (i === at ? { ...r, value: edit.value ?? "" } : r))
          else
            rows = [...rows, { ...blankRow(container), name: edit.name, value: edit.value ?? "" }]
        }
      }
    const blank = add || rows.length === 0 ? blankRow() : undefined
    const all = blank ? [...rows, blank] : rows
    setEditing(raw && !multi ? { rows, text: toDotenv(rows) } : { rows: all })
    setFocus(blank?.id)
  }

  const taken = new Map<string, Set<string>>()
  for (const { container, env } of referenced)
    taken.set(container, new Set([...(taken.get(container) ?? []), env.name]))
  const parsed = editing?.text === undefined ? undefined : parseDotenv(editing.text)
  const effective =
    editing && parsed
      ? fromDotenv(editing.rows, containers[0]?.name ?? "", parsed.entries)
      : (editing?.rows ?? [])
  const problems = problemsOf(effective, taken)
  const blocked =
    parsed && parsed.unread.length > 0
      ? t("vars.unread", parsed.unread)
      : problems.size > 0
        ? t("vars.blocked", problems.size)
        : undefined
  const held = editing
    ? changesOf(effective).map(({ container, changes }): Schemas["Change"] => ({
        op: "env",
        workload,
        container,
        changes,
      }))
    : undefined
  useHold(
    "variables",
    held ? { changes: held, blocked, editing: true, scope: { ops: ["env"], workload } } : undefined,
  )

  const setRows = (change: (rows: EnvRow[]) => EnvRow[]) =>
    setEditing((previous) => (previous ? { ...previous, rows: change(previous.rows) } : previous))
  const switchView = (next: "list" | "raw") => {
    if (!editing) {
      setRaw(next === "raw")
      return
    }
    if (next === "raw" && editing.text === undefined)
      setEditing({ rows: editing.rows, text: toDotenv(editing.rows) })
    if (next === "list" && editing.text !== undefined) {
      setEditing({ rows: effective })
      setRaw(false)
      return
    }
    setRaw(next === "raw")
  }
  const paste = (row: EnvRow, event: ClipboardEvent<HTMLInputElement>) => {
    const lines = pastedLines(event.clipboardData.getData("text"))
    if (!lines) return
    event.preventDefault()
    setRows((rows) => pasteInto(rows, row.id, lines, newId))
  }

  const revealButtons =
    secretNames.length > 0 ? (
      <span className="flex flex-wrap items-center gap-1.5">
        {secretNames.map((name) => {
          const state = secrets[name]
          return state?.values ? (
            <Button key={name} variant="outline" size="xs" onClick={() => hide(name)}>
              <EyeSlashIcon aria-hidden="true" data-icon="inline-start" />
              {t("vars.hide", name)}
            </Button>
          ) : (
            <Button
              key={name}
              variant="outline"
              size="xs"
              onClick={() => void reveal(name)}
              disabled={state?.pending}
            >
              {state?.pending ? (
                <Spinner data-icon="inline-start" />
              ) : (
                <EyeIcon aria-hidden="true" data-icon="inline-start" />
              )}
              {t("vars.reveal", name)}
            </Button>
          )
        })}
        <Hint label={t("vars.secretsHint")}>{t("vars.revealHint")}</Hint>
      </span>
    ) : null

  const sourceRow = ({ container, env }: { container: string; env: Schemas["EnvVar"] }) => {
    const value = valueOf(env)
    const secret = env.source.kind === "Secret"
    return (
      <li
        key={`${container}/${env.name}`}
        className="grid grid-cols-1 items-center gap-x-4 gap-y-1 py-2 sm:grid-cols-variable-actions"
      >
        <span className="font-mono text-xs font-medium" translate="no">
          {env.name}
        </span>
        <span className="flex min-w-0 items-center gap-1">
          {value !== undefined ? (
            <>
              <span className="truncate font-mono text-xs" translate="no">
                {value}
              </span>
              <CopyButton value={value} />
            </>
          ) : secret ? (
            <span className="inline-flex items-center gap-1.5 font-mono text-xs tracking-widest text-muted-foreground">
              <KeyIcon className="size-3.5" aria-hidden="true" />
              ••••••••
            </span>
          ) : (
            <span className="text-xs text-muted-foreground">{t("vars.atRuntime")}</span>
          )}
        </span>
        <span className="flex items-center justify-end gap-1">
          {env.pendingRestart ? <PendingMark pending={env.pendingRestart} /> : null}
          <SourceLink
            source={env.source}
            onOpen={(kind, name) => setEditor({ kind, name, key: env.source.key ?? undefined })}
          />
        </span>
      </li>
    )
  }

  const kubeFacts: [ReactNode, ReactNode][] = [
    ...(literal.length > 0 ? [[t("vars.direct"), "env[].value"] as [ReactNode, ReactNode]] : []),
    ...(fromObjects.length > 0
      ? [
          [
            t("vars.fromObjects"),
            [
              ...new Set(
                fromObjects.map(({ env }) =>
                  env.source.kind === "Secret"
                    ? "env[].valueFrom.secretKeyRef"
                    : "env[].valueFrom.configMapKeyRef",
                ),
              ),
            ].join(", "),
          ] as [ReactNode, ReactNode],
        ]
      : []),
    ...(imported.length > 0 ? [[t("vars.imported"), "envFrom"] as [ReactNode, ReactNode]] : []),
    ...(fromCluster.length > 0
      ? [
          [
            t("vars.fromCluster"),
            [
              ...new Set(
                fromCluster.map(({ env }) =>
                  env.source.kind === "ResourceField"
                    ? "env[].valueFrom.resourceFieldRef"
                    : "env[].valueFrom.fieldRef",
                ),
              ),
            ].join(", "),
          ] as [ReactNode, ReactNode],
        ]
      : []),
  ]

  const viewOptions = [
    {
      value: "list" as const,
      label: (
        <>
          <ListBulletsIcon className="size-3.5" aria-hidden="true" />
          {t("vars.list")}
        </>
      ),
    },
    {
      value: "raw" as const,
      label: (
        <>
          <BracketsCurlyIcon className="size-3.5" aria-hidden="true" />
          .env
        </>
      ),
    },
  ]
  const showRaw = editing ? editing.text !== undefined : raw
  const edited = editing ? editedCount(effective) : 0

  return (
    <div className="flex flex-col gap-4">
      <SectionHeader
        title={t("vars.title")}
        count={total > 0 ? total : undefined}
        description={t("vars.about")}
        action={
          <>
            {(editing && !multi) || (!editing && total > 0) ? (
              <Segmented
                label={t("vars.view")}
                value={showRaw ? "raw" : "list"}
                onChange={switchView}
                options={viewOptions}
              />
            ) : null}
            {editing || !draft ? null : (
              <ManagedTip tool={managed}>
                <Button
                  variant="outline"
                  size="sm"
                  disabled={Boolean(managed)}
                  onClick={() => startEditing(literal.length === 0)}
                >
                  <PencilSimpleIcon aria-hidden="true" data-icon="inline-start" />
                  {t("vars.edit")}
                </Button>
              </ManagedTip>
            )}
          </>
        }
      />
      <RefusedKinds unreadable={refused} line />
      {pending[0] ? (
        <Alert variant="warn" role="status">
          <ClockCounterClockwiseIcon aria-hidden="true" />
          <AlertDescription>
            <span className="flex flex-wrap items-center gap-x-3 gap-y-1.5">
              <span className="min-w-0 flex-1">
                {t("env.pendingRestart")}
                {t("common.colon")}
                {t("env.pendingRestartDetail", pending[0].pod)}
              </span>
              <ManagedTip tool={managed}>
                <Button variant="outline" size="sm" disabled={Boolean(managed)} onClick={restart}>
                  {t("title.restart", service.name)}
                </Button>
              </ManagedTip>
              {reloads ? (
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() =>
                    void navigate({
                      href: `/projects/${encodeURIComponent(project)}/services/${service.kind}/${encodeURIComponent(service.name)}?tab=settings#compute`,
                    })
                  }
                >
                  {t("env.restartByItself")}
                </Button>
              ) : null}
            </span>
          </AlertDescription>
        </Alert>
      ) : null}
      {secretNames.map((name) =>
        secrets[name]?.problem ? (
          <Problem
            key={name}
            problem={secrets[name].problem}
            retry={() => void reveal(name)}
            what={t("refused.reveal", name)}
          />
        ) : null,
      )}

      {editing && editing.text !== undefined ? (
        <div className="flex flex-col gap-2">
          <Textarea
            mono
            size="sm"
            aria-label={t("vars.dotenv")}
            aria-invalid={blocked !== undefined}
            rows={Math.max(6, editing.text.split("\n").length + 1)}
            value={editing.text}
            onChange={(event) => setEditing({ rows: editing.rows, text: event.target.value })}
          />
          <p className="text-xs text-secondary">{t("vars.dotenvHint")}</p>
          {[...problems].map(([id, problem]) => {
            const row = effective.find((r) => r.id === id)
            return row ? (
              <p key={id} className="text-xs text-fail-strong">
                <span className="font-mono" translate="no">
                  {row.name || "="}
                </span>{" "}
                {problemText(t, problem)}
              </p>
            ) : null
          })}
        </div>
      ) : total === 0 && !editing ? (
        <Empty variant="outline" size="inline">
          <EmptyMedia variant="glyph">
            <KeyIcon aria-hidden="true" />
          </EmptyMedia>
          <EmptyHeader>
            <EmptyTitle>{t("vars.emptyTitle")}</EmptyTitle>
            <EmptyDescription>{t("vars.emptyBody")}</EmptyDescription>
          </EmptyHeader>
          {draft ? (
            <EmptyContent>
              <ManagedTip tool={managed}>
                <Button
                  variant="outline"
                  size="sm"
                  disabled={Boolean(managed)}
                  onClick={() => startEditing(true)}
                >
                  <PlusIcon aria-hidden="true" data-icon="inline-start" />
                  {t("vars.add")}
                </Button>
              </ManagedTip>
            </EmptyContent>
          ) : null}
        </Empty>
      ) : raw && !editing ? (
        <>
          {revealButtons}
          <div className="relative rounded-lg border bg-muted/30">
            <CopyButton value={dotenv} className="absolute top-2 right-2" />
            <pre className="overflow-x-auto p-4 font-mono text-xs leading-6" translate="no">
              {dotenv || "# —"}
            </pre>
          </div>
        </>
      ) : (
        <>
          {long && !editing ? (
            <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
              <FilterField value={query} onChange={setQuery} label={t("vars.filter")} slash />
              {literal.length > 0 && elsewhere.length > 0 ? (
                <p className="type-meta text-secondary">
                  {t("vars.also")}
                  {elsewhere.map((part, i) => (
                    <span key={part.id}>
                      {i > 0 ? t("common.sep") : null}
                      <Button
                        variant="link"
                        size="xs"
                        onClick={() =>
                          document.getElementById(part.id)?.scrollIntoView({ block: "start" })
                        }
                      >
                        {part.text}
                      </Button>
                    </span>
                  ))}
                </p>
              ) : null}
            </div>
          ) : null}
          <div className="flex flex-col divide-y divide-v2-border overflow-hidden rounded-xl border border-v2-border bg-card">
            {editing ? (
              <VarGroup
                label={t("vars.direct")}
                action={
                  edited > 0 ? (
                    <span className="type-meta text-secondary">{t("vars.edited", edited)}</span>
                  ) : undefined
                }
              >
                <ul className="flex flex-col divide-y">
                  {editing.rows.map((row) => (
                    <EditRow
                      key={row.id}
                      row={row}
                      containers={multi ? containers.map((c) => c.name) : undefined}
                      problem={problems.get(row.id)}
                      focus={focus === row.id}
                      onChange={(next) =>
                        setRows((rows) => rows.map((r) => (r.id === row.id ? next : r)))
                      }
                      onDrop={() => setRows((rows) => rows.filter((r) => r.id !== row.id))}
                      onPaste={(event) => paste(row, event)}
                    />
                  ))}
                </ul>
                <div className="flex flex-wrap items-center gap-x-3 gap-y-1 py-2">
                  <Button
                    variant="ghost"
                    size="xs"
                    onClick={() => {
                      const row = blankRow()
                      setRows((rows) => [...rows, row])
                      setFocus(row.id)
                    }}
                  >
                    <PlusIcon aria-hidden="true" data-icon="inline-start" />
                    {t("vars.addRow")}
                  </Button>
                  <span className="text-xs text-tertiary">{t("vars.pasteHint")}</span>
                </div>
              </VarGroup>
            ) : literal.length > 0 ? (
              <VarGroup label={t("vars.direct")} count={long ? literal.length : undefined}>
                <div className={cn(long && "max-h-90 overflow-y-auto")}>
                  {long ? (
                    <div
                      aria-hidden="true"
                      className="sticky top-0 z-raised grid grid-cols-variable gap-x-4 border-b bg-card py-1.5 type-meta text-muted-foreground max-sm:hidden"
                    >
                      <span>{t("vars.name")}</span>
                      <span>{t("vars.value")}</span>
                    </div>
                  ) : null}
                  {shownLiteral.length === 0 ? (
                    <p className="py-2 text-xs text-muted-foreground">{t("vars.noMatch")}</p>
                  ) : null}
                  <ul className="flex flex-col divide-y">
                    {shownLiteral.map(({ container, env }) => {
                      const targets = named.get(`${container}/${env.name}`) ?? []
                      return (
                        <li
                          key={`${container}/${env.name}`}
                          className={cn(
                            "grid grid-cols-1 items-center gap-x-4 gap-y-1 py-2 sm:grid-cols-variable",
                            entrance(createdKey.env(workload, env.name)),
                          )}
                        >
                          <span
                            className="flex items-center gap-1.5 font-mono text-xs font-medium"
                            translate="no"
                          >
                            {env.name}
                            {multi ? (
                              <span className="font-normal text-muted-foreground">{container}</span>
                            ) : null}
                          </span>
                          <span className="flex min-w-0 items-center gap-2">
                            <span
                              className="truncate font-mono text-xs"
                              translate="no"
                              title={env.value ?? ""}
                            >
                              {env.value}
                            </span>
                            <CopyButton value={env.value ?? ""} />
                            {targets.map((target) => (
                              <Tooltip key={target} content={t("vars.names", target)}>
                                <span className="ml-auto inline-flex shrink-0 items-center gap-1 type-meta text-muted-foreground">
                                  <ArrowRightIcon className="size-3" aria-hidden="true" />
                                  {target}
                                </span>
                              </Tooltip>
                            ))}
                          </span>
                        </li>
                      )
                    })}
                  </ul>
                </div>
              </VarGroup>
            ) : null}

            {shownObjects.length + shownImported.length > 0 ? (
              <VarGroup id="vars-objects" label={t("vars.fromObjects")} action={revealButtons}>
                <ul className="flex flex-col divide-y">
                  {shownObjects.map(sourceRow)}
                  {shownImported.map(({ container, imported: i }) => (
                    <li
                      key={`${container}/${i.origin.fieldPath ?? i.source.name}`}
                      className="grid grid-cols-1 items-center gap-x-4 gap-y-1 py-2 sm:grid-cols-variable-actions"
                    >
                      <span
                        className="font-mono text-xs font-medium text-muted-foreground"
                        translate="no"
                      >
                        {i.prefix ? `${i.prefix}*` : t("vars.allKeys")}
                      </span>
                      <span className="text-xs text-muted-foreground">
                        {t("vars.keys", i.keyCount ?? 0)}
                      </span>
                      <span className="flex items-center justify-end gap-1">
                        {i.pendingRestart ? <PendingMark pending={i.pendingRestart} /> : null}
                        <SourceLink
                          source={i.source}
                          whole
                          onOpen={(kind, name) => setEditor({ kind, name })}
                        />
                      </span>
                    </li>
                  ))}
                </ul>
              </VarGroup>
            ) : null}

            {shownCluster.length > 0 ? (
              <VarGroup id="vars-cluster" label={t("vars.fromCluster")}>
                <ul className="flex flex-col divide-y">{shownCluster.map(sourceRow)}</ul>
              </VarGroup>
            ) : null}
          </div>
        </>
      )}
      <KubeDetails facts={kubeFacts} />
      {editor ? (
        <Suspense fallback={null}>
          <DataEditor
            project={project}
            kind={editor.kind}
            name={editor.name}
            highlight={editor.key}
            service={workload}
            onClose={() => setEditor(undefined)}
          />
        </Suspense>
      ) : null}
      {planner.dialog}
    </div>
  )
}

function EditRow({
  row,
  containers,
  problem,
  focus,
  onChange,
  onDrop,
  onPaste,
}: {
  row: EnvRow
  /** The containers a new row can go in, when there are several. */
  containers?: string[]
  problem?: RowProblem
  focus: boolean
  onChange: (row: EnvRow) => void
  onDrop: () => void
  onPaste: (event: ClipboardEvent<HTMLInputElement>) => void
}) {
  const t = useT()
  const state = rowState(row)
  const label = row.was?.name || row.name
  if (state === "removed")
    return (
      <li className="grid grid-cols-2 items-center gap-x-3 gap-y-1 py-2 sm:grid-cols-variable-edit">
        <span
          className="col-span-2 truncate font-mono text-xs text-tertiary line-through sm:col-span-1"
          translate="no"
        >
          {row.was?.name}
        </span>
        <span
          className="col-span-2 truncate font-mono text-xs text-tertiary line-through sm:col-span-1"
          translate="no"
        >
          {row.was?.value}
        </span>
        <span className="type-meta text-fail-strong">{t("vars.state.removed")}</span>
        <Button
          variant="ghost"
          size="xs"
          className="justify-self-end sm:justify-self-auto"
          onClick={() => onChange({ ...row, removed: false })}
        >
          <ArrowCounterClockwiseIcon aria-hidden="true" data-icon="inline-start" />
          {t("vars.undo")}
        </Button>
      </li>
    )
  const problemId = `${row.id}-problem`
  return (
    <li className="grid grid-cols-2 items-start gap-x-3 gap-y-1 py-2 sm:grid-cols-variable-edit">
      <span className="col-span-2 flex min-w-0 flex-col gap-1 sm:col-span-1">
        {containers && !row.was ? (
          <Select
            size="sm"
            value={row.container}
            onValueChange={(container) => onChange({ ...row, container })}
            aria-label={t("vars.container")}
            items={containers.map((name) => ({ value: name, label: name }))}
          />
        ) : null}
        <Input
          size="sm"
          mono
          placeholder={t("vars.namePlaceholder")}
          aria-label={t("vars.name")}
          aria-invalid={problem !== undefined}
          aria-describedby={problem ? problemId : undefined}
          value={row.name}
          // oxlint-disable-next-line jsx-a11y/no-autofocus -- the row was added on request, to be typed in
          autoFocus={focus}
          onChange={(event) => onChange({ ...row, name: event.target.value })}
          onPaste={onPaste}
        />
        {problem ? (
          <span id={problemId} className="text-xs text-fail-strong">
            {problemText(t, problem)}
          </span>
        ) : containers && row.was ? (
          <span className="text-xs text-tertiary" translate="no">
            {row.container}
          </span>
        ) : null}
      </span>
      <Input
        size="sm"
        mono
        className="col-span-2 sm:col-span-1"
        aria-label={t("vars.valueOf", label || t("vars.name"))}
        value={row.value}
        onChange={(event) => onChange({ ...row, value: event.target.value })}
      />
      <span
        className={cn(
          "self-center type-meta sm:self-start sm:pt-1",
          state === "added" ? "text-ok-strong" : state === "changed" ? "text-warn-strong" : "",
        )}
      >
        {state === "added" && (row.name || row.value)
          ? t("vars.state.added")
          : state === "changed"
            ? t("vars.state.changed")
            : null}
      </span>
      <Button
        variant="muted"
        size="icon-xs"
        className="justify-self-end sm:justify-self-auto"
        aria-label={t("edit.remove", label)}
        title={t("edit.remove", label)}
        onClick={() => (row.was ? onChange({ ...row, removed: true }) : onDrop())}
      >
        <TrashIcon aria-hidden="true" />
      </Button>
    </li>
  )
}

function problemText(t: Translate, problem: RowProblem): string {
  return t(`vars.problem.${problem}`)
}

function PendingMark({ pending }: { pending: Schemas["PendingRestart"] }) {
  const t = useT()
  return (
    <Tooltip content={t("env.pendingRestartDetail", pending.pod)}>
      <span className="flex items-center gap-1 text-xs text-warn-strong">
        <ClockCounterClockwiseIcon className="size-3.5" aria-hidden="true" />
        <span className="sr-only">{t("env.pendingRestart")}</span>
      </span>
    </Tooltip>
  )
}

function VarGroup({
  id,
  label,
  count,
  action,
  children,
}: {
  id?: string
  label: string
  count?: number
  action?: ReactNode
  children: ReactNode
}) {
  return (
    <section id={id} className="flex scroll-mt-4 flex-col px-5 pt-3 pb-1">
      <div className="flex flex-wrap items-center gap-2 pb-1">
        <h3 className="type-meta font-medium text-secondary">
          {label}
          {count !== undefined ? (
            <span className="ml-1.5 text-muted-foreground">{count}</span>
          ) : null}
        </h3>
        {action ? <span className="ml-auto">{action}</span> : null}
      </div>
      {children}
    </section>
  )
}

/** Where a value comes from, in words; for a config file or secret, a link to edit it there. */
function SourceLink({
  source,
  whole = false,
  onOpen,
}: {
  source: Source
  whole?: boolean
  onOpen: (kind: "ConfigMap" | "Secret", name: string) => void
}) {
  const t = useT()
  const words = sourceWords(t, source, whole)
  const kind = source.kind
  const name = source.name
  if ((kind !== "ConfigMap" && kind !== "Secret") || !name)
    return <span className="truncate text-xs text-secondary">{words}</span>
  return (
    <Tooltip content={t("vars.editThere")}>
      <Button variant="link" size="xs" className="min-w-0" onClick={() => onOpen(kind, name)}>
        <span className="truncate">{words}</span>
      </Button>
    </Tooltip>
  )
}

const fieldWords = {
  "metadata.name": "vars.field.podName",
  "metadata.namespace": "vars.field.namespace",
  "status.podIP": "vars.field.podIP",
  "status.hostIP": "vars.field.hostIP",
  "spec.nodeName": "vars.field.nodeName",
  "spec.serviceAccountName": "vars.field.serviceAccount",
  "limits.cpu": "vars.field.cpuLimit",
  "limits.memory": "vars.field.memoryLimit",
  "requests.cpu": "vars.field.cpuRequest",
  "requests.memory": "vars.field.memoryRequest",
} as const

type FieldKey = (typeof fieldWords)[keyof typeof fieldWords]

function sourceWords(t: Translate, source: Source, whole = false): string {
  const name = source.name ?? ""
  switch (source.kind) {
    case "Secret":
      return whole || !source.key
        ? t("vars.allOfSecret", name)
        : t("vars.inSecret", name, source.key)
    case "ConfigMap":
      return whole || !source.key
        ? t("vars.allOfConfig", name)
        : t("vars.inConfig", name, source.key)
    case "PodField":
    case "ResourceField": {
      const path = source.fieldPath ?? ""
      const key = (fieldWords as Record<string, FieldKey | undefined>)[path]
      return key ? t(key) : t("vars.field.other", path)
    }
    case "File":
      return t("vars.fromFile", source.fieldPath ?? name)
    case "Literal":
      return ""
  }
}
