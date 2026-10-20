import { useForm } from "@tanstack/react-form"
import { useQuery } from "@tanstack/react-query"
import { lazy, Suspense, useState, type ReactElement } from "react"
import ArrowRightIcon from "~icons/ph/arrow-right"
import BracketsCurlyIcon from "~icons/ph/brackets-curly"
import EyeIcon from "~icons/ph/eye"
import EyeSlashIcon from "~icons/ph/eye-slash"
import FileTextIcon from "~icons/ph/file-text"
import KeyIcon from "~icons/ph/key"
import ListBulletsIcon from "~icons/ph/list-bullets"
import PencilSimpleIcon from "~icons/ph/pencil-simple"
import PlusIcon from "~icons/ph/plus"
import TrashIcon from "~icons/ph/trash"
import TreeStructureIcon from "~icons/ph/tree-structure"

import { api, toProblem, unwrap, type Problem as ProblemBody, type Schemas } from "@/api/client"
import { configMapsQuery } from "@/api/queries"
import { CopyButton, Hint, Panel, Segmented } from "@/components/bits"
import { FormField } from "@/components/form-field"
import { usePlanner } from "@/components/plan/planner"
import { Problem } from "@/components/problem"
import { RefusedKinds } from "@/components/refused"
import { Button } from "@zeabur/design-system/components/button"
import { Input } from "@zeabur/design-system/components/input"
import { Select } from "@zeabur/design-system/components/select"
import { Spinner } from "@zeabur/design-system/components/spinner"
import { Tooltip } from "@zeabur/design-system/components/tooltip"
import { useT } from "@/i18n"
import type { Edge } from "@/model/connections"
import { configObjectsOf, type Service } from "@/model/service"

const DataEditor = lazy(() =>
  import("@/components/data-editor").then((m) => ({ default: m.DataEditor })),
)

type Revealed = Record<string, Schemas["SecretValue"]>

interface Draft {
  container: string
  name: string
  value: string
  was?: string
}

/** Kubernetes also accepts dots and dashes, which a shell cannot read. */
const nameRule = /^[-._a-zA-Z][-._a-zA-Z0-9]*$/

interface SecretState {
  values?: Revealed
  pending?: boolean
  problem?: ProblemBody
}

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
  const [draft, setDraft] = useState<Draft>()
  const [editor, setEditor] = useState<{
    kind: "ConfigMap" | "Secret"
    name: string
    key?: string
  }>()
  const planner = usePlanner(project)
  const managed = service.provenance.tool ?? undefined
  const workload = { kind: service.kind, name: service.name }
  const planEnv = (container: string, changes: Schemas["EnvEdit"][]) =>
    planner.plan({
      title: t("title.env", service.name),
      changes: [{ op: "env", workload, container, changes }],
    })
  const configMapList = useQuery(configMapsQuery(project))
  const configMaps = new Map((configMapList.data?.items ?? []).map((c) => [c.origin.name, c.data]))
  const named = new Map<string, string[]>()
  for (const edge of edges)
    for (const evidence of edge.evidence)
      if (evidence.basis === "envValue") {
        const key = `${evidence.container}/${evidence.variable}`
        named.set(key, [...(named.get(key) ?? []), evidence.service])
      }

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
      return `${env.name}=${value === undefined ? (env.source.kind === "Secret" ? "••••••••" : `# ${describe(env.source)}`) : quote(value)}`
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
        : [`# ${i.source.kind} ${name}: ${t("vars.keys", i.keyCount ?? 0)}`]
    }),
  ].join("\n")

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center gap-2">
        <div className="flex flex-wrap items-center gap-2">
          {secretNames.map((name) => {
            const state = secrets[name]
            return state?.values ? (
              <Button key={name} variant="outline" size="sm" onClick={() => hide(name)}>
                <EyeSlashIcon aria-hidden="true" data-icon="inline-start" />
                {t("vars.hide", name)}
              </Button>
            ) : (
              <Button
                key={name}
                variant="outline"
                size="sm"
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
          {secretNames.length > 0 ? (
            <Hint label={t("vars.secretsHint")}>{t("vars.revealHint")}</Hint>
          ) : null}
        </div>
        <span className="ml-auto">
          <Segmented
            label={t("vars.view")}
            value={raw ? "raw" : "list"}
            onChange={(value) => setRaw(value === "raw")}
            options={[
              {
                value: "list",
                label: (
                  <>
                    <ListBulletsIcon className="size-3.5" aria-hidden="true" />
                    {t("vars.list")}
                  </>
                ),
              },
              {
                value: "raw",
                label: (
                  <>
                    <BracketsCurlyIcon className="size-3.5" aria-hidden="true" />
                    .env
                  </>
                ),
              },
            ]}
          />
        </span>
      </div>
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

      {raw ? (
        <div className="relative rounded-lg border bg-muted/30">
          <CopyButton value={dotenv} className="absolute top-2 right-2" />
          <pre className="overflow-x-auto p-4 font-mono text-xs leading-6" translate="no">
            {dotenv || "# —"}
          </pre>
        </div>
      ) : (
        <>
          <Panel
            title={t("vars.definedHere")}
            action={
              <ManagedTip tool={managed}>
                <Button
                  variant="outline"
                  size="xs"
                  disabled={Boolean(managed) || draft !== undefined}
                  onClick={() =>
                    setDraft({ container: containers[0]?.name ?? "", name: "", value: "" })
                  }
                >
                  <PlusIcon aria-hidden="true" data-icon="inline-start" />
                  {t("vars.add")}
                </Button>
              </ManagedTip>
            }
          >
            {draft && draft.was === undefined ? (
              <DraftRow
                draft={draft}
                containers={containers.map((c) => c.name)}
                onCancel={() => setDraft(undefined)}
                onReview={(done) => {
                  planEnv(done.container, [
                    { name: done.name.trim(), op: "set", value: done.value },
                  ])
                  setDraft(undefined)
                }}
              />
            ) : null}
            {literal.length === 0 ? (
              draft ? null : (
                <p className="text-sm text-muted-foreground">{t("vars.none")}</p>
              )
            ) : (
              <ul className="-my-1 flex flex-col divide-y">
                {literal.map(({ container, env }) => {
                  const targets = named.get(`${container}/${env.name}`) ?? []
                  if (draft?.was === env.name && draft.container === container)
                    return (
                      <li key={`${container}/${env.name}`} className="py-2">
                        <DraftRow
                          draft={draft}
                          containers={[container]}
                          onCancel={() => setDraft(undefined)}
                          onReview={(done) => {
                            planEnv(container, [{ name: env.name, op: "set", value: done.value }])
                            setDraft(undefined)
                          }}
                        />
                      </li>
                    )
                  return (
                    <li
                      key={`${container}/${env.name}`}
                      className="grid grid-cols-1 items-center gap-x-4 gap-y-1 py-2 sm:grid-cols-variable-actions"
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
                      <span className="flex items-center justify-end gap-0.5">
                        <ManagedTip tool={managed}>
                          <Button
                            variant="muted"
                            size="icon-xs"
                            disabled={Boolean(managed) || draft !== undefined}
                            aria-label={t("edit.edit", env.name)}
                            title={t("edit.edit", env.name)}
                            onClick={() =>
                              setDraft({
                                container,
                                name: env.name,
                                value: env.value ?? "",
                                was: env.name,
                              })
                            }
                          >
                            <PencilSimpleIcon aria-hidden="true" />
                          </Button>
                        </ManagedTip>
                        <ManagedTip tool={managed}>
                          <Button
                            variant="muted"
                            size="icon-xs"
                            disabled={Boolean(managed)}
                            aria-label={t("edit.remove", env.name)}
                            title={t("edit.remove", env.name)}
                            onClick={() => planEnv(container, [{ name: env.name, op: "remove" }])}
                          >
                            <TrashIcon aria-hidden="true" />
                          </Button>
                        </ManagedTip>
                      </span>
                    </li>
                  )
                })}
              </ul>
            )}
          </Panel>

          <Panel title={t("vars.fromObjects")} bodyClassName="flex flex-col gap-3">
            <RefusedKinds unreadable={refused} line />
            {referenced.length + imported.length === 0 ? (
              refused.length > 0 ? null : (
                <p className="text-sm text-muted-foreground">{t("vars.none")}</p>
              )
            ) : (
              <ul className="-my-1 flex flex-col divide-y">
                {referenced.map(({ container, env }) => {
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
                          <span className="text-xs text-muted-foreground">
                            {t("vars.atRuntime")}
                          </span>
                        )}
                      </span>
                      <span className="flex items-center justify-end gap-1">
                        <SourceChip source={env.source} />
                        <EditSource
                          source={env.source}
                          onOpen={(kind, name) =>
                            setEditor({ kind, name, key: env.source.key ?? undefined })
                          }
                        />
                      </span>
                    </li>
                  )
                })}
                {imported.map(({ container, imported: i }) => (
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
                      <SourceChip source={i.source} />
                      <EditSource
                        source={i.source}
                        onOpen={(kind, name) => setEditor({ kind, name })}
                      />
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </Panel>
        </>
      )}
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

function ManagedTip({ tool, children }: { tool?: string; children: ReactElement }) {
  const t = useT()
  if (!tool) return children
  return (
    <Tooltip content={t("ops.managedEdit", tool)}>
      <span>{children}</span>
    </Tooltip>
  )
}

function EditSource({
  source,
  onOpen,
}: {
  source: Schemas["ValueSource"]
  onOpen: (kind: "ConfigMap" | "Secret", name: string) => void
}) {
  const t = useT()
  const kind = source.kind
  const name = source.name
  if ((kind !== "ConfigMap" && kind !== "Secret") || !name) return null
  return (
    <Button
      variant="muted"
      size="icon-xs"
      aria-label={t("vars.editIn", kind, name)}
      title={t("vars.editIn", kind, name)}
      onClick={() => onOpen(kind, name)}
    >
      <PencilSimpleIcon aria-hidden="true" />
    </Button>
  )
}

function DraftRow({
  draft,
  containers,
  onCancel,
  onReview,
}: {
  draft: Draft
  containers: string[]
  onCancel: () => void
  onReview: (draft: Draft) => void
}) {
  const t = useT()
  const form = useForm({
    defaultValues: draft,
    onSubmit: ({ value }) => {
      if (nameRule.test(value.name.trim())) onReview(value)
    },
  })
  return (
    <form
      className="flex flex-col gap-2 pb-2 sm:flex-row sm:items-center"
      onSubmit={(event) => {
        event.preventDefault()
        void form.handleSubmit()
      }}
    >
      {draft.was === undefined && containers.length > 1 ? (
        <form.Field name="container">
          {(f) => (
            <FormField label={t("vars.container")} hideLabel className="sm:w-32">
              <Select
                size="sm"
                value={f.state.value}
                onValueChange={(name) => f.handleChange(name)}
                className="w-full"
                items={containers.map((name) => ({ value: name, label: name }))}
              />
            </FormField>
          )}
        </form.Field>
      ) : null}
      <form.Field name="name">
        {(f) => (
          <FormField label={t("vars.name")} hideLabel className="sm:w-56">
            <Input
              size="sm"
              mono
              placeholder={t("vars.name")}
              value={f.state.value}
              readOnly={draft.was !== undefined}
              // oxlint-disable-next-line jsx-a11y/no-autofocus -- the row opens on request, to be typed in
              autoFocus={draft.was === undefined}
              onChange={(event) => f.handleChange(event.target.value)}
              onBlur={f.handleBlur}
            />
          </FormField>
        )}
      </form.Field>
      <form.Field name="value">
        {(f) => (
          <FormField label={t("vars.value")} hideLabel className="flex-1">
            <Input
              size="sm"
              mono
              placeholder={t("vars.value")}
              value={f.state.value}
              // oxlint-disable-next-line jsx-a11y/no-autofocus -- the row opens on request, to be typed in
              autoFocus={draft.was !== undefined}
              onChange={(event) => f.handleChange(event.target.value)}
              onBlur={f.handleBlur}
            />
          </FormField>
        )}
      </form.Field>
      <span className="flex shrink-0 items-center gap-1.5">
        <Button type="button" variant="ghost" size="xs" onClick={onCancel}>
          {t("edit.cancel")}
        </Button>
        <form.Subscribe selector={(s) => nameRule.test(s.values.name.trim())}>
          {(valid) => (
            <Button type="submit" size="xs" disabled={!valid}>
              {t("edit.review")}
            </Button>
          )}
        </form.Subscribe>
      </span>
    </form>
  )
}

function quote(value: string): string {
  return /[\s#"']/.test(value) ? JSON.stringify(value) : value
}

function describe(source: Schemas["ValueSource"]): string {
  return [source.kind, source.name, source.key ?? source.fieldPath].filter(Boolean).join(" ")
}

function SourceChip({
  source,
}: {
  source: Schemas["ValueSource"] | Schemas["ImportedEnv"]["source"]
}) {
  const icon =
    source.kind === "Secret" ? (
      <KeyIcon className="size-3" aria-hidden="true" />
    ) : source.kind === "ConfigMap" ? (
      <FileTextIcon className="size-3" aria-hidden="true" />
    ) : (
      <TreeStructureIcon className="size-3" aria-hidden="true" />
    )
  const detail =
    "key" in source && source.key
      ? source.key
      : "fieldPath" in source && source.fieldPath
        ? source.fieldPath
        : ""
  return (
    <span
      className="inline-flex w-fit max-w-full items-center gap-1 type-meta text-muted-foreground"
      translate="no"
      title={`${source.kind} ${source.name ?? ""} ${detail}`.trim()}
    >
      {icon}
      <span>{source.kind}</span>
      <span className="truncate font-mono text-foreground/80">{source.name ?? ""}</span>
      {detail ? <span className="truncate font-mono">{detail}</span> : null}
    </span>
  )
}
