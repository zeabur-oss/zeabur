// A ConfigMap's or Secret's keys, edited in place: the key a variable reads is highlighted, a
// change is planned and applied like every other write, and the plan names every service that
// reads the object. A Secret's values stay hidden until revealed (the same request and right as
// the Variables tab); a new value can be typed without revealing the old one.
import { useForm } from "@tanstack/react-form"
import { useQuery } from "@tanstack/react-query"
import { useState } from "react"
import ArrowCounterClockwiseIcon from "~icons/ph/arrow-counter-clockwise"
import EyeIcon from "~icons/ph/eye"
import GitBranchIcon from "~icons/ph/git-branch"
import PlusIcon from "~icons/ph/plus"
import TrashIcon from "~icons/ph/trash"
import XIcon from "~icons/ph/x"

import { api, toProblem, unwrap, type Problem as ProblemBody, type Schemas } from "@/api/client"
import { configMapsQuery, objectQuery, projectQuery } from "@/api/queries"
import { FormField } from "@/components/form-field"
import { LoadingRows } from "@/components/page"
import { usePlanner } from "@/components/plan/planner"
import { Problem } from "@/components/problem"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Dialog, DialogClose, DialogContent, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Spinner } from "@/components/ui/spinner"
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"
import { useT } from "@/i18n"
import { useScrollOnce } from "@/lib/in-view"
import { addressOf } from "@/lib/objects"
import { cn } from "@/lib/utils"

type Kind = "ConfigMap" | "Secret"
type Edit = { op: "set"; value: string } | { op: "remove" }

/** What the person changed: edits of the keys there are (by key), new keys, and whether the
 * readers restart after the write. */
interface Draft {
  edits: Record<string, Edit>
  added: { key: string; value: string }[]
  restartAfter: boolean
}

const blank: Draft = { edits: {}, added: [], restartAfter: false }

/** The writes a draft asks for: the edits, then the new keys that have a name. */
function changesOf({ edits, added }: Draft): Schemas["DataEdit"][] {
  return [
    ...Object.entries(edits).map(([key, edit]): Schemas["DataEdit"] =>
      edit.op === "set" ? { key, op: "set", value: edit.value } : { key, op: "remove" },
    ),
    ...added
      .filter((row) => row.key.trim())
      .map((row): Schemas["DataEdit"] => ({ key: row.key.trim(), op: "set", value: row.value })),
  ]
}

/** A Secret's keys, read from its YAML (values hidden): the lines under `data:` and `stringData:`. */
function secretKeys(yaml: string): string[] {
  const keys: string[] = []
  let inData = false
  for (const line of yaml.split("\n")) {
    if (/^(data|stringData):\s*$/.test(line)) inData = true
    else if (/^\S/.test(line)) inData = false
    else if (inData) {
      const match = line.match(/^ {2}("?)([^":]+)\1:/)
      if (match?.[2]) keys.push(match[2])
    }
  }
  return [...new Set(keys)]
}

export function DataEditor({
  project,
  kind,
  name,
  highlight,
  service,
  onClose,
}: {
  project: string
  kind: Kind
  name: string
  /** The key a variable reads, shown highlighted. */
  highlight?: string
  /** The service the editor was opened from. */
  service?: Schemas["WorkloadRef"]
  onClose: () => void
}) {
  const t = useT()
  const document = useQuery(projectQuery(project))
  const resource = document.data?.resources.find((r) => r.kind === kind && r.name === name)
  const configMaps = useQuery({ ...configMapsQuery(project), enabled: kind === "ConfigMap" })
  const secretYaml = useQuery({
    ...objectQuery(
      project,
      addressOf("Secret", name) ?? { group: "", version: "v1", resource: "secrets", name },
    ),
    enabled: kind === "Secret",
  })
  const [revealed, setRevealed] = useState<Record<string, string>>()
  const [revealing, setRevealing] = useState<{ pending?: boolean; problem?: ProblemBody }>({})
  const planner = usePlanner(project, onClose)
  const form = useForm({
    defaultValues: blank,
    onSubmit: ({ value }) =>
      planner.plan({
        title: t("title.data", kind, name),
        changes: [{ op: "data", kind, name, changes: changesOf(value) }],
        restartAfter: value.restartAfter,
        service,
      }),
  })
  const scrollTo = useScrollOnce<HTMLLIElement>()

  const configMap = configMaps.data?.items.find((c) => c.origin.name === name)
  const values: Record<string, string | undefined> =
    kind === "ConfigMap"
      ? (configMap?.data ?? {})
      : Object.fromEntries(
          secretKeys(secretYaml.data?.yaml ?? "").map((key) => [key, revealed?.[key]]),
        )
  const binary = kind === "ConfigMap" ? (configMap?.binaryKeys ?? []) : []
  const keys = Object.keys(values).toSorted()
  const pending = kind === "ConfigMap" ? configMaps.isPending : secretYaml.isPending
  const error = kind === "ConfigMap" ? configMaps.error : secretYaml.error
  const managed = resource?.provenance.tool ?? undefined
  const readers = resource?.attribution.owners ?? []

  const reveal = async () => {
    setRevealing({ pending: true })
    try {
      const answer = unwrap(
        await api.POST("/api/v1/projects/{project}/secrets/{name}/reveal", {
          params: { path: { project, name } },
          cache: "no-store",
        }),
      )
      setRevealed(Object.fromEntries(Object.entries(answer.values).map(([k, v]) => [k, v.value])))
      setRevealing({})
    } catch (failure) {
      setRevealing({ problem: toProblem(failure) })
    }
  }

  return (
    <Dialog open onOpenChange={(open) => (open ? undefined : onClose())}>
      <DialogContent variant="panel">
        <div className="flex items-center gap-2 border-b px-5 py-3">
          <DialogTitle className="flex min-w-0 flex-1 items-baseline">
            <span className="truncate font-mono" translate="no">
              {name}
            </span>
            <span className="ml-2 shrink-0 type-meta font-normal text-muted-foreground">
              {kind}
            </span>
          </DialogTitle>
          {managed ? (
            <Badge variant="outline">
              <GitBranchIcon aria-hidden="true" />
              {t("common.managedBy", managed)}
            </Badge>
          ) : null}
          <DialogClose
            render={<Button variant="ghost" size="icon-sm" aria-label={t("common.close")} />}
          >
            <XIcon aria-hidden="true" />
          </DialogClose>
        </div>
        <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-5">
          {readers.length > 0 ? (
            <p className="flex flex-wrap items-center gap-1.5 type-meta text-muted-foreground">
              {t("data.readers")}
              {readers.map((w) => (
                <Badge key={`${w.kind}/${w.name}`} variant="outline" translate="no">
                  {w.name}
                </Badge>
              ))}
            </p>
          ) : null}
          {managed ? (
            <p className="rounded-lg border border-dashed px-3 py-2 text-sm text-muted-foreground">
              {t("ops.managedEdit", managed)}
            </p>
          ) : null}
          {kind === "Secret" && !revealed ? (
            <div className="flex items-center gap-2">
              <Button
                variant="outline"
                size="sm"
                onClick={() => void reveal()}
                disabled={revealing.pending}
              >
                {revealing.pending ? (
                  <Spinner data-icon="inline-start" />
                ) : (
                  <EyeIcon aria-hidden="true" data-icon="inline-start" />
                )}
                {t("data.reveal")}
              </Button>
              <span className="type-meta text-muted-foreground">{t("yaml.hidden")}</span>
            </div>
          ) : null}
          {revealing.problem ? (
            <Problem problem={revealing.problem} retry={() => void reveal()} />
          ) : null}
          {pending ? (
            <LoadingRows label={t("common.loading")} rows={4} />
          ) : error ? (
            <Problem error={error} />
          ) : (
            <ul className="flex flex-col divide-y rounded-xl border" aria-label={`${kind} ${name}`}>
              <form.Field name="edits">
                {(f) =>
                  keys.map((key) => {
                    const edits = f.state.value
                    const edit = edits[key]
                    const removed = edit?.op === "remove"
                    const current = values[key]
                    const shown = edit?.op === "set" ? edit.value : (current ?? "")
                    const hit = key === highlight
                    /** The edits with `key`'s set to `next`, or dropped when it is undefined. */
                    const withEdit = (next: Edit | undefined) => {
                      const { [key]: _dropped, ...rest } = edits
                      return next ? { ...rest, [key]: next } : rest
                    }
                    return (
                      <li
                        key={key}
                        ref={hit ? scrollTo : undefined}
                        data-highlight={hit ? "" : undefined}
                        className={cn("flex flex-col gap-1.5 px-3 py-2.5", hit && "bg-brand-soft")}
                      >
                        <span className="flex items-center gap-2">
                          <span
                            className={cn(
                              "min-w-0 flex-1 truncate font-mono text-xs font-medium",
                              removed && "line-through opacity-60",
                            )}
                            translate="no"
                          >
                            {key}
                          </span>
                          <Button
                            variant="muted"
                            size="icon-xs"
                            disabled={Boolean(managed)}
                            aria-label={removed ? t("edit.undo", key) : t("edit.remove", key)}
                            title={removed ? t("edit.undo", key) : t("edit.remove", key)}
                            onClick={() =>
                              f.handleChange(withEdit(removed ? undefined : { op: "remove" }))
                            }
                          >
                            {removed ? (
                              <ArrowCounterClockwiseIcon aria-hidden="true" />
                            ) : (
                              <TrashIcon aria-hidden="true" />
                            )}
                          </Button>
                        </span>
                        <FormField label={key} hideLabel>
                          <Textarea
                            size="sm"
                            mono
                            rows={Math.min(6, Math.max(1, shown.split("\n").length))}
                            value={shown}
                            placeholder={
                              kind === "Secret" && current === undefined && !edit
                                ? "••••••••"
                                : undefined
                            }
                            disabled={Boolean(managed) || removed}
                            onChange={(event) => {
                              const value = event.target.value
                              f.handleChange(
                                withEdit(value === current ? undefined : { op: "set", value }),
                              )
                            }}
                          />
                        </FormField>
                      </li>
                    )
                  })
                }
              </form.Field>
              {binary.map((key) => (
                <li key={key} className="flex items-center gap-2 px-3 py-2.5">
                  <span className="min-w-0 flex-1 truncate font-mono text-xs" translate="no">
                    {key}
                  </span>
                  <span className="type-meta text-muted-foreground">{t("data.binary")}</span>
                </li>
              ))}
              <form.Field name="added" mode="array">
                {(rows) =>
                  rows.state.value.map((row, index) => (
                    // oxlint-disable-next-line react/no-array-index-key -- new rows have no key yet
                    <li key={index} className="flex flex-col gap-1.5 px-3 py-2.5">
                      <span className="flex items-center gap-2">
                        <form.Field name={`added[${index}].key`}>
                          {(f) => (
                            <FormField label={t("data.key")} hideLabel className="flex-1">
                              <Input
                                size="sm"
                                mono
                                placeholder={t("data.key")}
                                value={f.state.value}
                                onChange={(event) => f.handleChange(event.target.value)}
                              />
                            </FormField>
                          )}
                        </form.Field>
                        <Button
                          variant="muted"
                          size="icon-xs"
                          aria-label={t("edit.remove", row.key)}
                          onClick={() => rows.removeValue(index)}
                        >
                          <TrashIcon aria-hidden="true" />
                        </Button>
                      </span>
                      <form.Field name={`added[${index}].value`}>
                        {(f) => (
                          <FormField label={t("vars.value")} hideLabel>
                            <Textarea
                              size="sm"
                              mono
                              placeholder={t("vars.value")}
                              rows={1}
                              value={f.state.value}
                              onChange={(event) => f.handleChange(event.target.value)}
                            />
                          </FormField>
                        )}
                      </form.Field>
                    </li>
                  ))
                }
              </form.Field>
            </ul>
          )}
          <div>
            <Button
              variant="outline"
              size="sm"
              disabled={Boolean(managed)}
              onClick={() => form.pushFieldValue("added", { key: "", value: "" })}
            >
              <PlusIcon aria-hidden="true" data-icon="inline-start" />
              {t("data.addKey")}
            </Button>
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-3 border-t px-5 py-3">
          <form.Field name="restartAfter">
            {(f) => (
              <label className="flex items-center gap-2 text-sm">
                <Switch
                  checked={f.state.value}
                  onCheckedChange={f.handleChange}
                  disabled={Boolean(managed)}
                />
                {t("data.restartAfter")}
              </label>
            )}
          </form.Field>
          <form.Subscribe selector={(s) => changesOf(s.values).length}>
            {(count) => (
              <span className="ml-auto flex items-center gap-2">
                {count === 0 ? (
                  <span className="type-meta text-muted-foreground">{t("data.noChanges")}</span>
                ) : null}
                <Button
                  size="sm"
                  disabled={count === 0 || Boolean(managed)}
                  onClick={() => void form.handleSubmit()}
                >
                  {t("edit.review")}
                </Button>
              </span>
            )}
          </form.Subscribe>
        </div>
        {planner.dialog}
      </DialogContent>
    </Dialog>
  )
}
