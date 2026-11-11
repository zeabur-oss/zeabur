import { useForm } from "@tanstack/react-form"
import { useQuery } from "@tanstack/react-query"
import { useState, type Ref } from "react"
import ArrowCounterClockwiseIcon from "~icons/ph/arrow-counter-clockwise"
import DownloadSimpleIcon from "~icons/ph/download-simple"
import EyeIcon from "~icons/ph/eye"
import EyeSlashIcon from "~icons/ph/eye-slash"
import GitBranchIcon from "~icons/ph/git-branch"
import PlusIcon from "~icons/ph/plus"
import TrashIcon from "~icons/ph/trash"
import XIcon from "~icons/ph/x"

import { ReviewWord } from "@/components/plan/draft"
import type { Schemas } from "@/api/client"
import { configMapsQuery, objectQuery, projectQuery } from "@/api/queries"
import { CopyButton } from "@/components/bits"
import { FormField } from "@/components/form-field"
import { LoadingRows } from "@/components/page"
import { GateNote, useGates } from "@/components/gate"
import { usePlanner } from "@/components/plan/planner"
import { Problem } from "@/components/problem"
import { Badge } from "@zeabur-oss/design-system/components/badge"
import { Button } from "@zeabur-oss/design-system/components/button"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogTitle,
} from "@zeabur-oss/design-system/components/dialog"
import { Input } from "@zeabur-oss/design-system/components/input"
import { Textarea } from "@zeabur-oss/design-system/components/textarea"
import { useT } from "@/i18n"
import { kindName } from "@/i18n/kind-name"
import { useScrollOnce } from "@/lib/in-view"
import { addressOf } from "@/lib/objects"
import { decodeSecretValue, secretData } from "@/lib/secret-data"
import { cn } from "@/lib/utils"

type Kind = "ConfigMap" | "Secret"
type Edit = { op: "set"; value: string } | { op: "remove" }

interface Draft {
  edits: Record<string, Edit>
  added: { key: string; value: string }[]
}

const blank: Draft = { edits: {}, added: [] }

/** What typing `value` into a key's input changes; clearing the input of a hidden value is none. */
export function editAfterTyping(current: string, value: string, hidden: boolean): Edit | undefined {
  return value === current || (hidden && value === "") ? undefined : { op: "set", value }
}

/** A Secret's keys from its YAML: text values decoded, the rest as bytes. */
export function secretKeys(yaml: string): {
  text: Record<string, string>
  binary: Record<string, Uint8Array<ArrayBuffer>>
} {
  const text: Record<string, string> = {}
  const binary: Record<string, Uint8Array<ArrayBuffer>> = {}
  for (const [key, base64] of Object.entries(secretData(yaml))) {
    const value = decodeSecretValue(base64)
    if (value?.text === undefined) binary[key] = value?.bytes ?? new Uint8Array()
    else text[key] = value.text
  }
  return { text, binary }
}

function download(name: string, bytes: Uint8Array<ArrayBuffer>) {
  const href = URL.createObjectURL(new Blob([bytes], { type: "application/octet-stream" }))
  const link = document.createElement("a")
  link.href = href
  link.download = name
  link.click()
  window.setTimeout(() => URL.revokeObjectURL(href), 1000)
}

export function dataChangesOf({ edits, added }: Draft): Schemas.DataEdit[] {
  return [
    ...Object.entries(edits).map(([key, edit]): Schemas.DataEdit =>
      edit.op === "set" ? { key, op: "set", value: edit.value } : { key, op: "remove" },
    ),
    ...added
      .filter((row) => row.key.trim())
      .map((row): Schemas.DataEdit => ({ key: row.key.trim(), op: "set", value: row.value })),
  ]
}

function ValueActions({
  name,
  current,
  bytes,
  hidden,
  secret,
  onToggle,
}: {
  name: string
  current: string
  bytes: Uint8Array<ArrayBuffer> | undefined
  hidden: boolean
  secret: boolean
  onToggle: () => void
}) {
  const t = useT()
  if (bytes)
    return (
      <>
        <span className="type-meta text-muted-foreground">{t("data.bytes", bytes.length)}</span>
        <Button
          variant="muted"
          size="icon-xs"
          aria-label={t("data.download", name)}
          title={t("data.download", name)}
          onClick={() => download(name, bytes)}
        >
          <DownloadSimpleIcon aria-hidden="true" />
        </Button>
      </>
    )
  if (!secret) return null
  const label = hidden ? t("data.show", name) : t("data.hide", name)
  return (
    <>
      <Button variant="muted" size="icon-xs" aria-label={label} title={label} onClick={onToggle}>
        {hidden ? <EyeIcon aria-hidden="true" /> : <EyeSlashIcon aria-hidden="true" />}
      </Button>
      <CopyButton value={current} />
    </>
  )
}

function RemoveButton({
  name,
  removed,
  locked,
  onClick,
}: {
  name: string
  removed: boolean
  locked: boolean
  onClick: () => void
}) {
  const t = useT()
  const label = removed ? t("edit.undo", name) : t("edit.remove", name)
  return (
    <Button
      variant="muted"
      size="icon-xs"
      disabled={locked}
      aria-label={label}
      title={label}
      onClick={onClick}
    >
      {removed ? (
        <ArrowCounterClockwiseIcon aria-hidden="true" />
      ) : (
        <TrashIcon aria-hidden="true" />
      )}
    </Button>
  )
}

function ValueInput({
  name,
  current,
  edit,
  hidden,
  disabled,
  onEdit,
}: {
  name: string
  current: string
  edit: Edit | undefined
  hidden: boolean
  disabled: boolean
  onEdit: (next: Edit | undefined) => void
}) {
  const typed = edit?.op === "set" ? edit.value : undefined
  const shown = typed ?? (hidden ? "" : current)
  return (
    <FormField label={name} hideLabel>
      <Textarea
        size="sm"
        mono
        rows={Math.min(6, Math.max(1, shown.split("\n").length))}
        value={shown}
        placeholder={hidden && !edit ? "••••••••" : undefined}
        disabled={disabled}
        onChange={(event) => onEdit(editAfterTyping(current, event.target.value, hidden))}
      />
    </FormField>
  )
}

interface KeyRowProps {
  name: string
  current: string
  bytes: Uint8Array<ArrayBuffer> | undefined
  hidden: boolean
  secret: boolean
  edit: Edit | undefined
  locked: boolean
  highlight: boolean
  rowRef: Ref<HTMLLIElement> | undefined
  onToggle: () => void
  onEdit: (next: Edit | undefined) => void
}

function KeyRow({ name, bytes, edit, locked, highlight, rowRef, onEdit, ...value }: KeyRowProps) {
  const removed = edit?.op === "remove"
  return (
    <li
      ref={rowRef}
      data-highlight={highlight ? "" : undefined}
      className={cn("flex flex-col gap-1.5 px-3 py-2.5", highlight && "bg-brand-soft")}
    >
      <span className="flex items-center gap-2">
        <span
          className={cn(
            "min-w-0 flex-1 truncate font-mono text-xs font-medium",
            removed && "line-through opacity-60",
          )}
          translate="no"
        >
          {name}
        </span>
        <ValueActions name={name} bytes={bytes} {...value} />
        <RemoveButton
          name={name}
          removed={removed}
          locked={locked}
          onClick={() => onEdit(removed ? undefined : { op: "remove" })}
        />
      </span>
      {bytes ? null : (
        <ValueInput
          name={name}
          current={value.current}
          edit={edit}
          hidden={value.hidden}
          disabled={locked || removed}
          onEdit={onEdit}
        />
      )}
    </li>
  )
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
  highlight?: string
  service?: Schemas.WorkloadRef
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
  const [visible, setVisible] = useState<ReadonlySet<string>>(new Set())
  const planner = usePlanner(project, onClose)
  const form = useForm({
    defaultValues: blank,
    onSubmit: ({ value }) => {
      const staged = planner.plan({
        title: t("title.data", kindName(t, kind), name),
        changes: [{ op: "data", kind, name, changes: dataChangesOf(value) }],
        service,
      })
      if (staged) onClose()
    },
  })
  const scrollTo = useScrollOnce<HTMLLIElement>()

  const configMap = configMaps.data?.items.find((c) => c.origin.name === name)
  const secret = secretKeys(kind === "Secret" ? (secretYaml.data?.yaml ?? "") : "")
  const values: Record<string, string> =
    kind === "ConfigMap" ? (configMap?.data ?? {}) : secret.text
  const binary = kind === "ConfigMap" ? (configMap?.binaryKeys ?? []) : []
  const keys = [...Object.keys(values), ...Object.keys(secret.binary)].toSorted()
  const pending = kind === "ConfigMap" ? configMaps.isPending : secretYaml.isPending
  const error = kind === "ConfigMap" ? configMaps.error : secretYaml.error
  const managed = resource?.provenance.tool ?? undefined
  const gates = useGates(project)
  const gate = gates(kind === "Secret" ? "changeSecrets" : "changeConfig")
  const locked = Boolean(managed) || !gate.open
  const readers = resource?.attribution.owners ?? []

  const toggle = (key: string) =>
    setVisible((shown) => {
      const next = new Set(shown)
      if (!next.delete(key)) next.add(key)
      return next
    })

  return (
    <Dialog open onOpenChange={(open) => (open ? undefined : onClose())}>
      <DialogContent variant="panel">
        <div className="flex items-center gap-2 border-b px-6 py-3">
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
            render={
              <Button
                variant="ghost"
                size="icon-sm"
                className="-mr-1"
                aria-label={t("common.close")}
              />
            }
          >
            <XIcon aria-hidden="true" />
          </DialogClose>
        </div>
        <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-6 py-5">
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
          ) : (
            <GateNote gate={gate} />
          )}
          {pending ? (
            <LoadingRows label={t("common.loading")} rows={4} />
          ) : error ? (
            <Problem error={error} />
          ) : (
            <ul className="flex flex-col divide-y rounded-lg border" aria-label={`${kind} ${name}`}>
              <form.Field name="edits">
                {(f) =>
                  keys.map((key) => {
                    const edits = f.state.value
                    const hit = key === highlight
                    return (
                      <KeyRow
                        key={key}
                        name={key}
                        current={values[key] ?? ""}
                        bytes={secret.binary[key]}
                        hidden={kind === "Secret" && !visible.has(key)}
                        secret={kind === "Secret"}
                        edit={edits[key]}
                        locked={locked}
                        highlight={hit}
                        rowRef={hit ? scrollTo : undefined}
                        onToggle={() => toggle(key)}
                        onEdit={(next) => {
                          const { [key]: _dropped, ...rest } = edits
                          f.handleChange(next ? { ...rest, [key]: next } : rest)
                        }}
                      />
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
              disabled={locked}
              onClick={() => form.pushFieldValue("added", { key: "", value: "" })}
            >
              <PlusIcon aria-hidden="true" data-icon="inline-start" />
              {t("data.addKey")}
            </Button>
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-3 border-t px-6 py-3">
          <form.Subscribe selector={(s) => dataChangesOf(s.values).length}>
            {(count) => (
              <span className="ml-auto flex items-center gap-2">
                {count === 0 ? (
                  <span className="type-meta text-muted-foreground">{t("data.noChanges")}</span>
                ) : null}
                <Button
                  size="sm"
                  disabled={count === 0 || locked}
                  onClick={() => void form.handleSubmit()}
                >
                  <ReviewWord />
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
