// Restoring a volume from its K8up backups: into a new volume the service can switch to, in
// place for a StatefulSet's volume, or out to a file to download.
import { useQuery } from "@tanstack/react-query"
import { useState } from "react"
import ArrowCounterClockwiseIcon from "~icons/ph/arrow-counter-clockwise"
import DownloadSimpleIcon from "~icons/ph/download-simple"
import WarningIcon from "~icons/ph/warning"

import { type Schemas } from "@/api/client"
import { volumeRestoresQuery } from "@/api/queries"
import { Ago } from "@/components/bits"
import { FormField, NameField } from "@/components/form-field"
import { GateTip, useGate } from "@/components/gate"
import { PlanDrawer, ReviewFooter, ReviewForm } from "@/components/plan/plan-drawer"
import { type PlanAsk, usePlanner, usePlanStep } from "@/components/plan/planner"
import { StatusPill } from "@/components/status"
import { type Translate, useI18n, useT } from "@/i18n"
import type { Messages } from "@/i18n/en"
import { formatTime } from "@/lib/format"
import { outcomeHealth } from "@/model/cluster-backups"
import { restoredName } from "@/model/database-restore"
import {
  exportSnapshot,
  volumeExportUrl,
  restoreInPlace,
  restoreToNewVolume,
  swapToVolume,
} from "@/model/volume-backups"
import { Alert, AlertDescription } from "@zeabur/design-system/components/alert"
import { Button, buttonVariants } from "@zeabur/design-system/components/button"
import { Checkbox } from "@zeabur/design-system/components/checkbox"
import { Label } from "@zeabur/design-system/components/label"
import { Select } from "@zeabur/design-system/components/select"

type Service = Schemas["Service"]
type Mount = Schemas["Mount"]
type Restores = Schemas["VolumeRestores"]
type Restore = Schemas["VolumeRestore"]
type Mode = "new" | "inPlace" | "export"
type Template = Parameters<typeof restoreToNewVolume>[0]

const NAME_MAX = 63
const SHOWN = 5

/** A volume's restores, and restoring it from a backup point. */
export function VolumeRestore({
  project,
  service,
  claim,
  mount,
}: {
  project: string
  service: Service
  claim: string
  mount?: Mount
}) {
  const t = useT()
  const data = useQuery(volumeRestoresQuery(project, claim)).data
  const gate = useGate(project, "restoreVolumes")
  const [mode, setMode] = useState<Mode | undefined>(undefined)
  if (!data || data.state !== "installed" || !data.template) return null
  // A StatefulSet's volumes come from its claim templates: a new claim cannot take their place.
  const stateful = service.kind === "StatefulSet"
  const points = data.snapshots.length > 0
  return (
    <div className="flex flex-col gap-1.5 border-t border-rule py-2.5">
      <div className="flex flex-wrap items-center gap-2">
        <span className="min-w-48 flex-1 type-meta text-muted-foreground">
          {points ? t("volumeRestore.points", claim) : t("volumeRestore.noPoints", claim)}
        </span>
        {points ? (
          <>
            <GateTip gate={gate}>
              <Button
                size="xs"
                variant="outline"
                onClick={() => setMode(stateful ? "inPlace" : "new")}
              >
                <ArrowCounterClockwiseIcon aria-hidden="true" data-icon="inline-start" />
                {stateful ? t("volumeRestore.inPlace") : t("volumeRestore.new")}
              </Button>
            </GateTip>
            {data.export ? (
              <GateTip gate={gate}>
                <Button size="xs" variant="ghost" onClick={() => setMode("export")}>
                  {t("volumeRestore.export")}
                </Button>
              </GateTip>
            ) : null}
          </>
        ) : null}
      </div>
      {points ? (
        <div className="flex flex-col gap-0.5 type-meta text-muted-foreground">
          <p>{t("volumeRestore.warnOverwrite")}</p>
          <p>{t("volumeRestore.warnRepo")}</p>
        </div>
      ) : null}
      {data.restores.length > 0 ? (
        <ul className="flex flex-col">
          {data.restores.slice(0, SHOWN).map((r) => (
            <RestoreRow
              key={r.name}
              project={project}
              service={service}
              claim={claim}
              restore={r}
            />
          ))}
        </ul>
      ) : null}
      {mode ? (
        <RestoreDrawer
          project={project}
          service={service}
          claim={claim}
          mount={mount}
          data={data}
          mode={mode}
          onClose={() => setMode(undefined)}
        />
      ) : null}
    </div>
  )
}

function RestoreRow({
  project,
  service,
  claim,
  restore,
}: {
  project: string
  service: Service
  claim: string
  restore: Restore
}) {
  const t = useT()
  const done = restore.outcome === "succeeded"
  const into = restore.into
  return (
    <li className="flex flex-col gap-1 border-t border-rule py-2 first:border-t-0">
      <div className="flex min-w-0 flex-wrap items-center gap-2.5">
        <span className="min-w-0 flex-1 truncate type-meta" translate="no">
          {restoreWords(t, claim, into)}
        </span>
        <StatusPill health={outcomeHealth[restore.outcome]} size="sm">
          {t(`backups.${restore.outcome}`)}
        </StatusPill>
        {restore.started ? (
          <Ago iso={restore.started} className="shrink-0 type-meta text-muted-foreground" />
        ) : null}
      </div>
      {restore.outcome === "failed" && restore.message ? (
        <p className="type-meta text-fail-strong" translate="no">
          {restore.message}
        </p>
      ) : null}
      {done ? <Done project={project} service={service} claim={claim} restore={restore} /> : null}
    </li>
  )
}

function restoreWords(t: Translate, claim: string, into: string | null | undefined) {
  if (!into) return t("volumeRestore.toFile")
  return into === claim ? t("volumeRestore.toItself", claim) : t("volumeRestore.toVolume", into)
}

/** The next step after a restore finished: download the file, start the service again, or
 * switch it to the new volume. */
function Done({
  project,
  service,
  claim,
  restore,
}: {
  project: string
  service: Service
  claim: string
  restore: Restore
}) {
  const t = useT()
  const into = restore.into
  if (!into)
    return (
      <div className="flex flex-wrap items-center gap-2">
        <a
          className={buttonVariants({ size: "xs", variant: "outline" })}
          href={volumeExportUrl(project, claim, restore.name)}
          download
        >
          <DownloadSimpleIcon aria-hidden="true" data-icon="inline-start" />
          {t("volumeRestore.download")}
        </a>
      </div>
    )
  return (
    <>
      <p className="type-meta text-muted-foreground">{t("volumeRestore.check")}</p>
      <div className="flex flex-wrap items-center gap-2">
        {into === claim ? (
          <StartAgain project={project} service={service} replicas={restore.replicas ?? 1} />
        ) : (
          <Swap project={project} service={service} claim={claim} into={into} />
        )}
      </div>
    </>
  )
}

function Swap({
  project,
  service,
  claim,
  into,
}: {
  project: string
  service: Service
  claim: string
  into: string
}) {
  const t = useT()
  const planner = usePlanner(project)
  const gate = useGate(project, "changeStorage")
  return (
    <>
      <GateTip gate={gate}>
        <Button
          size="xs"
          variant="outline"
          onClick={() =>
            planner.plan({
              title: t("volumeRestore.swapTitle", service.name, into),
              changes: swapToVolume({ kind: service.kind, name: service.name }, claim, into),
              notes: [
                t("volumeRestore.swapDowntime", service.name),
                t("volumeRestore.keep", claim),
              ],
            })
          }
        >
          {t("volumeRestore.swap")}
        </Button>
      </GateTip>
      {planner.dialog}
    </>
  )
}

function StartAgain({
  project,
  service,
  replicas,
}: {
  project: string
  service: Service
  replicas: number
}) {
  const t = useT()
  const planner = usePlanner(project)
  const gate = useGate(project, "scale")
  if ((service.replicas?.desired ?? 0) > 0) return null
  return (
    <>
      <GateTip gate={gate}>
        <Button
          size="xs"
          variant="outline"
          onClick={() =>
            planner.plan({
              title: t("volumeRestore.startTitle", service.name),
              changes: [
                { op: "scale", workload: { kind: service.kind, name: service.name }, replicas },
              ],
            })
          }
        >
          {t("volumeRestore.start")}
        </Button>
      </GateTip>
      {planner.dialog}
    </>
  )
}

function RestoreDrawer({
  project,
  service,
  claim,
  mount,
  data,
  mode,
  onClose,
}: {
  project: string
  service: Service
  claim: string
  mount?: Mount
  data: Restores
  mode: Mode
  onClose: () => void
}) {
  const t = useT()
  const { locale } = useI18n()
  const step = usePlanStep(project, onClose)
  const { snapshot, setSnapshot, point, into, setInto, understood, setUnderstood } = useChoice(
    data,
    claim,
  )
  const [tried, setTried] = useState(false)
  const template = (data.template ?? {}) as Template
  const intoProblem = badName(into, claim)
  const title = t(TITLE[mode], claim)
  const ready = { new: !intoProblem, inPlace: understood, export: !!data.export }[mode]
  const review = () => {
    setTried(true)
    if (!point || !ready) return
    const ask = {
      template,
      point,
      when: formatTime(point.date, locale),
      claim,
      into,
      mount,
      service,
      exportTo: data.export as Record<string, unknown>,
    }
    step.plan({ title, ...PLANS[mode](t, ask, new Date()) })
  }
  return (
    <PlanDrawer
      step={step}
      onClose={onClose}
      title={title}
      description={t("volumeRestore.onlyBackups")}
      footer={
        <ReviewFooter
          onCancel={onClose}
          onReview={review}
          disabled={mode === "inPlace" && !understood}
        />
      }
    >
      <ReviewForm onReview={review}>
        <SnapshotField snapshots={data.snapshots} value={snapshot} onChange={setSnapshot} />
        {mode === "new" ? (
          <NameField
            label={t("volumeRestore.name")}
            hint={sameAs(t, mount)}
            error={tried && intoProblem ? t("volumeRestore.badName") : undefined}
            value={into}
            onChange={setInto}
          />
        ) : null}
        {mode === "inPlace" ? (
          <InPlaceWarning
            service={service.name}
            claim={claim}
            understood={understood}
            onUnderstood={setUnderstood}
          />
        ) : null}
      </ReviewForm>
    </PlanDrawer>
  )
}

function useChoice(data: Restores, claim: string) {
  const [snapshot, setSnapshot] = useState(data.snapshots[0]?.id ?? "")
  const [into, setInto] = useState(() => restoredName(claim, new Date(), NAME_MAX))
  const [understood, setUnderstood] = useState(false)
  const point = data.snapshots.find((s) => s.id === snapshot)
  return { snapshot, setSnapshot, point, into, setInto, understood, setUnderstood }
}

const badName = (name: string, claim: string) =>
  !/^[a-z0-9]([-a-z0-9]*[a-z0-9])?$/.test(name) || name === claim

const sameAs = (t: Translate, mount: Mount | undefined) =>
  t("volumeRestore.sameAs", mount?.size ?? "—", mount?.storageClass ?? "—")

function SnapshotField({
  snapshots,
  value,
  onChange,
}: {
  snapshots: Restores["snapshots"]
  value: string
  onChange: (id: string) => void
}) {
  const t = useT()
  const { locale } = useI18n()
  return (
    <FormField label={t("volumeRestore.point")} hint={t("volumeRestore.zone")}>
      <Select
        size="sm"
        className="max-w-full"
        value={value}
        onValueChange={onChange}
        items={snapshots.map((s) => ({
          value: s.id,
          label: `${formatTime(s.date, locale)}${t("common.sep")}${s.id.slice(0, 8)}`,
        }))}
      />
    </FormField>
  )
}

const TITLE = {
  new: "volumeRestore.newTitle",
  inPlace: "volumeRestore.inPlaceTitle",
  export: "volumeRestore.exportTitle",
} as const satisfies Record<Mode, keyof Messages>

type Ask = {
  template: Template
  point: { id: string }
  when: string
  claim: string
  into: string
  mount?: Mount
  service: Service
  exportTo: Record<string, unknown>
}

const PLANS: Record<Mode, (t: Translate, ask: Ask, now: Date) => Omit<PlanAsk, "title">> = {
  new: (t, a, now) => ({
    changes: restoreToNewVolume(a.template, a.point.id, a.claim, a.into, a.mount ?? {}, now),
    notes: [
      t("volumeRestore.newNote", a.into, a.when),
      t("volumeRestore.keep", a.claim),
      t("volumeRestore.asUser"),
    ],
  }),
  inPlace: (t, a, now) => ({
    changes: restoreInPlace(
      a.template,
      a.point.id,
      a.claim,
      { kind: a.service.kind, name: a.service.name },
      a.service.replicas?.desired || 1,
      now,
    ),
    notes: [t("volumeRestore.cleared", a.claim, a.when), t("volumeRestore.stops", a.service.name)],
  }),
  export: (t, a, now) => ({
    changes: exportSnapshot(a.template, a.exportTo, a.point.id, a.claim, now),
    notes: [t("volumeRestore.exportNote", a.when)],
  }),
}

/** What restoring in place costs, and the box that says it was read. */
function InPlaceWarning({
  service,
  claim,
  understood,
  onUnderstood,
}: {
  service: string
  claim: string
  understood: boolean
  onUnderstood: (on: boolean) => void
}) {
  const t = useT()
  return (
    <Alert variant="warn">
      <WarningIcon aria-hidden="true" />
      <AlertDescription>
        <ul className="flex list-disc flex-col gap-1 pl-4">
          <li>{t("volumeRestore.stops", service)}</li>
          <li>{t("volumeRestore.clearedAll", claim)}</li>
          <li>{t("volumeRestore.noUndo")}</li>
        </ul>
        <div className="mt-3 flex items-center gap-2">
          <Checkbox
            id="volume-restore-understood"
            checked={understood}
            onCheckedChange={(on) => onUnderstood(on === true)}
          />
          <Label htmlFor="volume-restore-understood">{t("volumeRestore.understood", claim)}</Label>
        </div>
      </AlertDescription>
    </Alert>
  )
}
