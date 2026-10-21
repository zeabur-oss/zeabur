import { useRef, useState, type ReactNode } from "react"
import ArrowCounterClockwiseIcon from "~icons/ph/arrow-counter-clockwise"
import CaretRightIcon from "~icons/ph/caret-right"
import FileTextIcon from "~icons/ph/file-text"
import HardDriveIcon from "~icons/ph/hard-drive"
import PencilSimpleIcon from "~icons/ph/pencil-simple"
import PlusIcon from "~icons/ph/plus"
import TrashIcon from "~icons/ph/trash"

import type { Schemas } from "@/api/client"
import { VolumeBackups } from "@/components/backups"
import { useVolumeFills, VolumeFullNote, VolumeUse } from "@/components/volume-use"
import { ManagedTip, Panel } from "@/components/bits"
import { FormField } from "@/components/form-field"
import { KubeDetails } from "@/components/kube-details"
import { useDraft, useHold } from "@/components/plan/draft"
import { StatusPill } from "@/components/status"
import { Button } from "@zeabur/design-system/components/button"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur/design-system/components/collapsible"
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
import { Switch } from "@zeabur/design-system/components/switch"
import { useT, type Translate } from "@/i18n"
import { createdKey, entrance } from "@/lib/created"
import { cn } from "@/lib/utils"
import { claimOf, type Service } from "@/model/service"
import { claimMounts } from "@/model/volume-use"
import {
  changesOf,
  claimNameOf,
  editedCount,
  nameOf,
  problemsOf,
  rowState,
  type VolumeProblem,
  type VolumeRow,
} from "@/model/volume-edit"
import { accessModes } from "@/routes/service/volume"

type Mount = Schemas["Mount"]

export function StorageTab({ project, service }: { project: string; service: Service }) {
  const t = useT()
  const draft = useDraft()
  const mounts = claimMounts(service)
  const volumes = useVolumeFills(
    project,
    service,
    mounts.map(({ mount }) => claimOf(mount)),
  )
  const workload = { kind: service.kind, name: service.name }
  const containers = service.containers.filter((c) => !c.init)
  const multi = containers.length > 1
  const managed = service.provenance.tool ?? undefined
  // A Job's template cannot change; a StatefulSet's volumes come from its claim templates.
  const editable = service.kind !== "Job" && draft !== null
  const addable = editable && service.kind !== "StatefulSet"
  const other = service.containers.flatMap((c) =>
    c.mounts
      .filter((m) => m.kind !== "PersistentVolumeClaim")
      .map((m) => ({ container: c.name, mount: m })),
  )
  const [rows, setRows] = useState<VolumeRow[]>()
  const [focus, setFocus] = useState<string>()
  const nextId = useRef(0)
  const blankRow = (): VolumeRow => ({
    id: `new-${nextId.current++}`,
    container: multi ? containers[0]?.name : undefined,
    mountPath: "",
    size: "1Gi",
    readOnly: false,
    removed: false,
    name: null,
    claimName: null,
    accessMode: "ReadWriteOnce",
  })
  const startEditing = (add: boolean) => {
    const existing = mounts.map(({ container, mount }): VolumeRow => ({
      id: `${container}/${mount.mountPath}`,
      container: multi ? container : undefined,
      mountPath: mount.mountPath,
      size: mount.size ?? "",
      readOnly: mount.readOnly ?? false,
      was: {
        mountPath: mount.mountPath,
        size: mount.size ?? "",
        readOnly: mount.readOnly ?? false,
        claim: claimOf(mount),
      },
      removed: false,
      name: null,
      claimName: null,
      accessMode: "ReadWriteOnce",
    }))
    const blank = add ? blankRow() : undefined
    setRows(blank ? [...existing, blank] : existing)
    setFocus(blank?.id)
  }
  const addRow = () => {
    const row = blankRow()
    setRows((previous) => [...(previous ?? []), row])
    setFocus(row.id)
  }
  const update = (id: string, change: (row: VolumeRow) => VolumeRow | undefined) =>
    setRows((previous) =>
      previous?.flatMap((row) => {
        if (row.id !== id) return [row]
        const next = change(row)
        return next ? [next] : []
      }),
    )

  const taken = new Set(other.map(({ mount }) => mount.mountPath))
  const problems = rows ? problemsOf(rows, taken) : new Map<string, VolumeProblem>()
  const edited = rows ? editedCount(rows) : 0
  useHold(
    "storage",
    rows
      ? {
          changes: changesOf(rows, workload),
          blocked: problems.size > 0 ? t("storage.blocked", problems.size) : undefined,
          editing: true,
          scope: {
            ops: ["addVolume", "expandVolume", "editMount", "removeVolume"],
            workload,
          },
        }
      : undefined,
  )

  const editButton =
    rows || !editable || mounts.length === 0 ? undefined : (
      <ManagedTip tool={managed}>
        <Button
          variant="outline"
          size="sm"
          disabled={managed !== undefined}
          onClick={() => startEditing(false)}
        >
          <PencilSimpleIcon aria-hidden="true" data-icon="inline-start" />
          {t("storage.edit")}
        </Button>
      </ManagedTip>
    )

  return (
    <div className="flex flex-col gap-8">
      <div className="flex flex-col gap-3">
        <SectionHeader
          title={t("storage.title")}
          count={mounts.length > 0 ? mounts.length : undefined}
          description={t("storage.about")}
          action={editButton}
        />
        {mounts.length === 0 && !rows ? (
          <Empty variant="outline" size="inline">
            <EmptyMedia variant="glyph">
              <HardDriveIcon aria-hidden="true" />
            </EmptyMedia>
            <EmptyHeader>
              <EmptyTitle>{t("storage.emptyTitle", service.name)}</EmptyTitle>
              <EmptyDescription>
                {addable
                  ? t("storage.emptyBody")
                  : service.kind === "StatefulSet"
                    ? t("form.statefulVolumes")
                    : t("storage.emptyJob")}
              </EmptyDescription>
            </EmptyHeader>
            {addable ? (
              <EmptyContent>
                <ManagedTip tool={managed}>
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={managed !== undefined}
                    onClick={() => startEditing(true)}
                  >
                    <PlusIcon aria-hidden="true" data-icon="inline-start" />
                    {t("form.addVolume")}
                  </Button>
                </ManagedTip>
              </EmptyContent>
            ) : null}
          </Empty>
        ) : (
          <div
            role="group"
            aria-label={t("storage.title")}
            className="flex flex-col overflow-hidden rounded-xl border border-v2-border bg-card"
          >
            <div
              aria-hidden="true"
              className="grid grid-cols-volume gap-x-3 border-b border-v2-border px-5 py-2 type-meta text-secondary max-sm:hidden"
            >
              <span>{t("storage.mountPath")}</span>
              <span>{t("form.size")}</span>
              <span>{t("form.readOnly")}</span>
              <span>{t("storage.status")}</span>
            </div>
            <ul className="flex flex-col divide-y divide-v2-border">
              {rows
                ? rows.map((row) => (
                    <EditRow
                      key={row.id}
                      row={row}
                      workload={service.name}
                      status={mounts.find((m) => m.mount.mountPath === row.was?.mountPath)?.mount}
                      problem={problems.get(row.id)}
                      focus={focus === row.id}
                      removable={addable}
                      onChange={(change) => update(row.id, change)}
                    />
                  ))
                : mounts.map(({ container, mount }) => {
                    const fill = volumes.fill(claimOf(mount))
                    return (
                      <li
                        key={`${container}/${mount.mountPath}`}
                        className={cn(
                          "grid grid-cols-2 items-center gap-x-3 gap-y-1 px-5 py-3 sm:grid-cols-volume",
                          entrance(createdKey.volume(workload, mount.mountPath)),
                        )}
                      >
                        <span className="col-span-2 flex min-w-0 flex-col gap-1.5 sm:col-span-1">
                          <span className="flex min-w-0 items-baseline gap-2">
                            <span
                              className="truncate font-mono type-body"
                              translate="no"
                              title={mount.mountPath}
                            >
                              {mount.mountPath}
                            </span>
                            {multi ? (
                              <span className="shrink-0 type-meta text-tertiary" translate="no">
                                {container}
                              </span>
                            ) : null}
                          </span>
                          {fill ? <VolumeUse fill={fill} className="max-w-64" /> : null}
                          {fill ? <VolumeFullNote fill={fill} /> : null}
                        </span>
                        <span className="font-mono type-body tabular-nums" translate="no">
                          {mount.size ?? "—"}
                        </span>
                        <span className="type-body text-secondary max-sm:justify-self-end">
                          {mount.readOnly ? t("storage.readOnly") : t("storage.writable")}
                        </span>
                        <span className="max-sm:col-span-2">
                          <Phase mount={mount} />
                        </span>
                      </li>
                    )
                  })}
            </ul>
            {rows ? (
              <div className="flex flex-wrap items-center gap-x-3 gap-y-1 border-t border-v2-border py-2 pr-5 pl-3">
                {addable ? (
                  <Button variant="ghost" size="xs" onClick={addRow}>
                    <PlusIcon aria-hidden="true" data-icon="inline-start" />
                    {t("form.addVolume")}
                  </Button>
                ) : null}
                {edited > 0 ? (
                  <span className="ml-auto type-meta text-secondary">
                    {t("vars.edited", edited)}
                  </span>
                ) : null}
              </div>
            ) : null}
          </div>
        )}
        {editable && !addable && mounts.length > 0 ? (
          <p className="type-meta text-secondary">{t("form.statefulVolumes")}</p>
        ) : null}
      </div>
      {other.length > 0 ? (
        <Panel title={t("storage.otherMounts")}>
          <ul className="flex flex-col divide-y">
            {other.map(({ container, mount }) => (
              <li
                key={`${container}/${mount.mountPath}`}
                className="flex items-center gap-2 py-2 text-xs"
              >
                <FileTextIcon className="size-3.5 text-muted-foreground" aria-hidden="true" />
                <span className="font-mono" translate="no">
                  {mount.mountPath}
                </span>
                <span className="ml-auto text-muted-foreground">{mountWords(t, mount)}</span>
              </li>
            ))}
          </ul>
        </Panel>
      ) : null}
      <VolumeBackups project={project} claims={mounts.map(({ mount }) => claimOf(mount))} />
      <KubeDetails
        facts={[
          ...mounts.map(({ mount: m }): [string, string] => [
            m.mountPath,
            [
              `PersistentVolumeClaim/${claimOf(m)}`,
              m.status,
              m.accessModes?.join(","),
              m.storageClass ? `storageClassName: ${m.storageClass}` : undefined,
            ]
              .filter(Boolean)
              .join(" "),
          ]),
          ...other.map(({ mount }): [string, string] => [
            mount.mountPath,
            mount.object
              ? `${mount.object.kind}/${mount.object.name}`
              : `${mount.kind} ${mount.name}`,
          ]),
        ]}
      />
    </div>
  )
}

function Phase({ mount }: { mount?: Mount }) {
  const t = useT()
  if (!mount?.status) return null
  return (
    <StatusPill health={mount.status === "Bound" ? "ok" : "warn"} size="sm">
      {phaseWord(mount.status, t)}
    </StatusPill>
  )
}

function EditRow({
  row,
  workload,
  status,
  problem,
  focus,
  removable,
  onChange,
}: {
  row: VolumeRow
  workload: string
  status?: Mount
  problem?: VolumeProblem
  focus: boolean
  removable: boolean
  onChange: (change: (row: VolumeRow) => VolumeRow | undefined) => void
}) {
  const t = useT()
  const state = rowState(row)
  const label = row.was?.mountPath ?? (row.mountPath.trim() || t("storage.newVolume"))
  const problemId = `${row.id}-problem`
  const invalid = (...kinds: VolumeProblem[]) => problem !== undefined && kinds.includes(problem)
  if (state === "removed")
    return (
      <li className="grid grid-cols-2 items-center gap-x-3 gap-y-1 px-5 py-3 sm:grid-cols-volume">
        <span
          className="col-span-2 truncate font-mono type-body text-tertiary line-through sm:col-span-1"
          translate="no"
        >
          {row.was?.mountPath}
        </span>
        <span className="font-mono type-body text-tertiary line-through" translate="no">
          {row.was?.size}
        </span>
        <span className="max-sm:hidden" />
        <span className="type-meta text-fail-strong">{t("vars.state.removed")}</span>
        <Button
          variant="ghost"
          size="icon-xs"
          aria-label={t("edit.undo", label)}
          title={t("vars.undo")}
          className="justify-self-end"
          onClick={() => onChange((r) => ({ ...r, removed: false }))}
        >
          <ArrowCounterClockwiseIcon aria-hidden="true" />
        </Button>
      </li>
    )
  const describedBy = problem ? problemId : undefined
  const fields = (
    <>
      <Input
        size="sm"
        mono
        className="col-span-2 sm:col-span-1"
        aria-label={row.was ? t("storage.pathOf", row.was.mountPath) : t("storage.newPath")}
        aria-invalid={invalid("badPath", "pathTaken")}
        aria-describedby={describedBy}
        placeholder={row.was ? undefined : "/data"}
        value={row.mountPath}
        // oxlint-disable-next-line jsx-a11y/no-autofocus -- the row was added on request, to be typed in
        autoFocus={focus}
        onChange={(event) => onChange((r) => ({ ...r, mountPath: event.target.value }))}
      />
      <Input
        size="sm"
        mono
        aria-label={t("storage.sizeOf", label)}
        aria-invalid={invalid("badSize", "shrinks")}
        aria-describedby={describedBy}
        value={row.size}
        onChange={(event) => onChange((r) => ({ ...r, size: event.target.value }))}
      />
      <span className="flex items-center max-sm:justify-self-end">
        {row.was ? (
          <Switch
            aria-label={t("storage.readOnlyOf", label)}
            checked={row.readOnly}
            onCheckedChange={(readOnly) => onChange((r) => ({ ...r, readOnly }))}
          />
        ) : null}
      </span>
      <span className="flex min-h-6 items-center">
        {state === "added" ? (
          <span className="type-meta text-ok-strong">{t("vars.state.added")}</span>
        ) : state === "changed" ? (
          <span className="type-meta text-warn-strong">{t("vars.state.changed")}</span>
        ) : (
          <Phase mount={status} />
        )}
      </span>
      {removable ? (
        <Button
          variant="muted"
          size="icon-xs"
          className="justify-self-end"
          aria-label={t("edit.remove", label)}
          title={t("edit.remove", label)}
          onClick={() => onChange((r) => (r.was ? { ...r, removed: true } : undefined))}
        >
          <TrashIcon aria-hidden="true" />
        </Button>
      ) : (
        <span />
      )}
    </>
  )
  const message = problem ? (
    <p id={problemId} className="col-span-full type-meta text-fail-strong">
      {t(`storage.problem.${problem}`)}
    </p>
  ) : row.was?.size !== undefined && row.size.trim() !== row.was.size ? (
    <p className="col-span-full type-meta text-secondary">{t("form.sizeHint")}</p>
  ) : null
  if (row.was)
    return (
      <li className="grid grid-cols-2 items-center gap-x-3 gap-y-1.5 px-5 py-2.5 sm:grid-cols-volume">
        {fields}
        {message}
      </li>
    )
  return (
    <li className="grid grid-cols-2 items-center gap-x-3 gap-y-1.5 px-5 py-2.5 sm:grid-cols-volume">
      {fields}
      {message}
      <Collapsible className="col-span-full">
        <CollapsibleTrigger variant="disclosure">
          <CaretRightIcon aria-hidden="true" />
          {t("form.advanced")}
        </CollapsibleTrigger>
        <CollapsibleContent>
          <NewVolumeDetails row={row} workload={workload} onChange={onChange} />
        </CollapsibleContent>
      </Collapsible>
    </li>
  )
}

function NewVolumeDetails({
  row,
  workload,
  onChange,
}: {
  row: VolumeRow
  workload: string
  onChange: (change: (row: VolumeRow) => VolumeRow | undefined) => void
}) {
  const t = useT()
  return (
    <div className="grid grid-cols-1 gap-3 pt-1 pb-1 sm:grid-cols-3">
      <Detail label={t("form.volumeName")}>
        <Input
          size="sm"
          mono
          value={nameOf(row)}
          onChange={(event) => onChange((r) => ({ ...r, name: event.target.value }))}
        />
      </Detail>
      <Detail label={t("form.claimName")} hint={t("form.claimNameHint")}>
        <Input
          size="sm"
          mono
          value={claimNameOf(workload, row)}
          onChange={(event) => onChange((r) => ({ ...r, claimName: event.target.value }))}
        />
      </Detail>
      <Detail label={t("form.accessMode")} hint={t("form.accessModeHint")}>
        <Select
          size="sm"
          value={row.accessMode}
          onValueChange={(accessMode) => onChange((r) => ({ ...r, accessMode }))}
          translate="no"
          items={accessModes.map((mode) => ({ value: mode, label: mode }))}
        />
      </Detail>
    </div>
  )
}

function Detail({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <FormField label={label} hint={hint}>
      {children}
    </FormField>
  )
}

function phaseWord(phase: string, t: Translate): string {
  if (phase === "Bound") return t("storage.bound")
  if (phase === "Pending") return t("storage.pending")
  if (phase === "Lost") return t("storage.lost")
  return phase
}

function mountWords(t: Translate, mount: Mount): string {
  const name = mount.object?.name ?? mount.name
  switch (mount.kind) {
    case "ConfigMap":
      return t("storage.fromConfig", name)
    case "Secret":
      return t("storage.fromSecret", name)
    case "EmptyDir":
    case "Ephemeral":
      return t("storage.scratch")
    case "DownwardAPI":
    case "Projected":
      return t("storage.fromCluster")
    default:
      return mount.name
  }
}
