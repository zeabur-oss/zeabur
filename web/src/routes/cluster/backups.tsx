// Cluster → Backups: Velero's backups of every object and the files in volumes. Restoring is
// left to the velero command-line tool.
import { useMutation, useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { useId, useState, type ReactNode } from "react"
import CaretRightIcon from "~icons/ph/caret-right"
import InfoIcon from "~icons/ph/info"
import WarningIcon from "~icons/ph/warning"

import { isStatus, type Schemas } from "@/api/client"
import {
  applyClusterBackups,
  clusterBackupsQuery,
  planClusterBackups,
  projectsQuery,
  queryClient,
} from "@/api/queries"
import { Ago } from "@/components/bits"
import { FormField } from "@/components/form-field"
import { GateTip, useHintGate } from "@/components/gate"
import { KubeDetails } from "@/components/kube-details"
import { Frame } from "@/components/members/frame"
import { LoadingRows } from "@/components/page"
import { COUNTDOWN_FROM_SECONDS, clock, useSecondsLeft } from "@/components/plan/expiry"
import { Problem } from "@/components/problem"
import { Schedule } from "@/components/schedule"
import { StatusDot, StatusPill } from "@/components/status"
import { YamlDiff } from "@/components/yaml"
import { useI18n, useT } from "@/i18n"
import { formatDate } from "@/lib/format"
import { useQuantityFormat } from "@/lib/quantity"
import { cn } from "@/lib/utils"
import {
  hourOf,
  insideCluster,
  lastGood,
  locationForm,
  locationProblems,
  locationRequest,
  mainLocation,
  mainSchedule,
  outcomeHealth,
  RESTORE_COMMAND,
  scheduleForm,
  scheduleProblems,
  scheduleRequest,
  takenAt,
  VELERO_COMPONENT,
  verdict,
  verdictHealth,
  type LocationForm,
  type ScheduleForm,
} from "@/model/cluster-backups"
import type { Gate } from "@/model/rights"
import { ClusterFrame } from "@/routes/cluster/frame"
import { Forbidden, type Role } from "@/routes/cluster/forbidden"
import { Section } from "@/routes/cluster/section"
import { Alert, AlertDescription } from "@zeabur/design-system/components/alert"
import { Button, buttonVariants } from "@zeabur/design-system/components/button"
import { Checkbox } from "@zeabur/design-system/components/checkbox"
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
import { Illustration } from "@zeabur/design-system/components/illustration"
import { Input } from "@zeabur/design-system/components/input"
import { Label } from "@zeabur/design-system/components/label"
import { RadioGroup, RadioOption } from "@zeabur/design-system/components/radio"
import { Select } from "@zeabur/design-system/components/select"

type Data = Schemas["ClusterBackups"]
type Request = Schemas["BackupsPlanRequest"]

export const backupsRole: Role = {
  name: "zeabur-cluster-backups",
  rules: [
    {
      group: "velero.io",
      resources: ["backupstoragelocations", "schedules", "backups", "podvolumebackups"],
    },
  ],
}

const refresh = () => {
  void queryClient.invalidateQueries({ queryKey: clusterBackupsQuery.queryKey })
}

export function BackupsPage() {
  const t = useT()
  const backups = useQuery(clusterBackupsQuery)
  const data = backups.data
  const refused = data?.state === "refused" || (backups.isError && isStatus(backups.error, 403))
  return (
    <ClusterFrame tab="backups">
      {refused ? (
        <Forbidden
          error={backups.error ?? undefined}
          what={t("clusterBackups.what")}
          missing={backupsRole.rules}
          role={backupsRole}
        />
      ) : backups.isPending ? (
        <LoadingRows label={t("cluster.backups")} rows={4} />
      ) : backups.isError ? (
        <Problem error={backups.error} />
      ) : data?.state === "not-installed" ? (
        <NotInstalled />
      ) : data ? (
        <Installed data={data} />
      ) : null}
    </ClusterFrame>
  )
}

function DatabasesNote() {
  const t = useT()
  return (
    <p className="flex items-start gap-1.5 type-meta text-muted-foreground">
      <InfoIcon aria-hidden="true" className="mt-0.5 size-3.5 shrink-0" />
      <span>{t("clusterBackups.databasesNote")}</span>
    </p>
  )
}

function NotInstalled() {
  const t = useT()
  return (
    <>
      <Empty variant="outline">
        <EmptyHeader>
          <EmptyMedia>
            <Illustration kind="empty" seed="backups" />
          </EmptyMedia>
          <EmptyTitle>{t("clusterBackups.notInstalled")}</EmptyTitle>
          <EmptyDescription>{t("clusterBackups.notInstalledBody")}</EmptyDescription>
        </EmptyHeader>
        <EmptyContent>
          <Link
            to="/cluster/components"
            search={{ component: VELERO_COMPONENT }}
            className={buttonVariants({ size: "sm" })}
          >
            {t("clusterBackups.install")}
          </Link>
        </EmptyContent>
      </Empty>
      <DatabasesNote />
    </>
  )
}

function Installed({ data }: { data: Data }) {
  const t = useT()
  const gate = useHintGate("changeBackups", data.canChange)
  const [plan, setPlan] = useState<{ request: Request; title: string }>()
  const open = (request: Request, title: string) => setPlan({ request, title })
  const location = mainLocation(data)
  const schedule = mainSchedule(data)
  return (
    <>
      <Verdict data={data} gate={gate} onPlan={open} />
      <LocationSection data={data} gate={gate} onPlan={open} />
      <ScheduleSection data={data} gate={gate} onPlan={open} />
      <BackupList data={data} />
      <div className="flex flex-col gap-3">
        <p className="type-meta text-muted-foreground">
          <WithCode text={t("clusterBackups.restore", RESTORE_COMMAND)} code={RESTORE_COMMAND} />
        </p>
        <KubeDetails
          facts={[
            [t("clusterBackups.kube.namespace"), data.namespace],
            ...(location
              ? ([
                  [
                    t("clusterBackups.kube.location"),
                    `BackupStorageLocation/${location.name} (${location.provider})`,
                  ],
                  [t("clusterBackups.kube.keys"), `Secret/${location.credential ?? "—"}`],
                ] as [string, string][])
              : []),
            ...data.schedules.map(
              (s) => [t("clusterBackups.kube.schedule"), `Schedule/${s.name}`] as [string, string],
            ),
            ...(schedule
              ? ([
                  [
                    t("clusterBackups.kube.fs"),
                    `defaultVolumesToFsBackup: ${String(schedule.fsBackup)}`,
                  ],
                ] as [string, string][])
              : []),
          ]}
        />
      </div>
      {plan ? (
        <PlanDialog request={plan.request} title={plan.title} onClose={() => setPlan(undefined)} />
      ) : null}
    </>
  )
}

/** A sentence with a command in it, set as code. */
function WithCode({ text, code }: { text: string; code: string }) {
  const [before, after] = text.split(code)
  return (
    <span>
      {before}
      <code className="type-mono text-foreground" translate="no">
        {code}
      </code>
      {after}
    </span>
  )
}

function Verdict({
  data,
  gate,
  onPlan,
}: {
  data: Data
  gate: Gate
  onPlan: (request: Request, title: string) => void
}) {
  const t = useT()
  const v = verdict(data)
  const good = lastGood(data.backups)
  const ready = mainLocation(data) !== undefined && mainSchedule(data) !== undefined
  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          <p className="flex items-center gap-2 type-body font-medium text-main" role="status">
            <StatusDot health={verdictHealth[v.as]} />
            {t(`clusterBackups.verdict.${v.as as "ok"}`)}
          </p>
          {good ? (
            <p className="pl-4 type-meta text-muted-foreground">
              {t("clusterBackups.lastGood")} <Ago iso={takenAt(good)} />
            </p>
          ) : null}
        </div>
        {ready ? (
          <GateTip gate={gate}>
            <Button
              size="sm"
              variant="outline"
              onClick={() => onPlan({ op: "backupNow" }, t("clusterBackups.plan.now"))}
            >
              {t("clusterBackups.now")}
            </Button>
          </GateTip>
        ) : null}
      </div>
      <DatabasesNote />
    </div>
  )
}

function Rows({ children }: { children: ReactNode }) {
  return (
    <dl className="flex flex-col divide-y divide-main rounded-lg border border-main bg-card">
      {children}
    </dl>
  )
}

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex min-w-0 flex-col gap-x-4 px-4 py-1.5 sm:flex-row sm:items-center">
      <dt className="shrink-0 type-meta text-muted-foreground sm:w-36">{label}</dt>
      <dd className="flex min-h-8 min-w-0 flex-1 flex-wrap items-center gap-x-2">{children}</dd>
    </div>
  )
}

const phaseHealth = { available: "ok", unavailable: "fail", unknown: "idle" } as const

function LocationSection({
  data,
  gate,
  onPlan,
}: {
  data: Data
  gate: Gate
  onPlan: (request: Request, title: string) => void
}) {
  const t = useT()
  const location = mainLocation(data)
  const [editing, setEditing] = useState(false)
  const form = editing || !location
  return (
    <Section
      id="backup-location"
      title={t("clusterBackups.location")}
      action={
        location && !editing ? (
          <GateTip gate={gate}>
            <Button size="xs" variant="outline" onClick={() => setEditing(true)}>
              {t("clusterBackups.edit")}
            </Button>
          </GateTip>
        ) : null
      }
    >
      {location && !editing ? (
        <>
          <Rows>
            <Row label={t("clusterBackups.bucket")}>
              <span className="min-w-0 truncate type-mono" translate="no">
                {location.prefix ? `${location.bucket}/${location.prefix}` : location.bucket}
              </span>
            </Row>
            <Row label={t("clusterBackups.endpoint")}>
              <span className="min-w-0 truncate type-mono" translate="no">
                {location.endpoint ?? t("clusterBackups.awsS3")}
              </span>
              {location.region ? (
                <span className="type-meta text-muted-foreground" translate="no">
                  {location.region}
                </span>
              ) : null}
            </Row>
            <Row label={t("clusterBackups.status")}>
              <StatusPill health={phaseHealth[location.phase]} size="sm">
                {t(`clusterBackups.phase.${location.phase}`)}
              </StatusPill>
              {location.lastValidated ? (
                <span className="type-meta text-muted-foreground">
                  {t("clusterBackups.checked")} <Ago iso={location.lastValidated} />
                </span>
              ) : null}
            </Row>
          </Rows>
          {location.phase === "unavailable" && location.message ? (
            <p className="type-meta break-words text-muted-foreground">{location.message}</p>
          ) : null}
          {location.endpoint && insideCluster(location.endpoint) ? <InsideWarning /> : null}
        </>
      ) : null}
      {form ? (
        <LocationEditor
          editing={location !== undefined}
          initial={locationForm(location)}
          gate={gate}
          onCancel={location ? () => setEditing(false) : undefined}
          onReview={(request) => {
            setEditing(false)
            onPlan(request, t("clusterBackups.plan.location"))
          }}
        />
      ) : null}
    </Section>
  )
}

function InsideWarning() {
  const t = useT()
  return (
    <Alert variant="warn">
      <WarningIcon aria-hidden="true" />
      <AlertDescription>{t("clusterBackups.inside")}</AlertDescription>
    </Alert>
  )
}

const formBox = "flex flex-col gap-4 rounded-xl border border-main bg-card p-5"

function LocationEditor({
  editing,
  initial,
  gate,
  onCancel,
  onReview,
}: {
  editing: boolean
  initial: LocationForm
  gate: Gate
  onCancel?: () => void
  onReview: (request: Request) => void
}) {
  const t = useT()
  const [form, setForm] = useState(initial)
  const [tried, setTried] = useState(false)
  const problems = locationProblems(form, editing)
  const set = (patch: Partial<LocationForm>) => setForm((v) => ({ ...v, ...patch }))
  const error = (problem: (typeof problems)[number]) =>
    tried && problems.includes(problem) ? [t(`clusterBackups.problem.${problem}`)] : []
  const text = (field: keyof LocationForm, placeholder?: string) => ({
    size: "sm" as const,
    mono: true,
    value: form[field],
    placeholder,
    autoComplete: "off",
    spellCheck: false,
    onChange: (event: { target: { value: string } }) => set({ [field]: event.target.value }),
  })
  return (
    <form
      className={formBox}
      onSubmit={(event) => {
        event.preventDefault()
        setTried(true)
        if (problems.length === 0) onReview(locationRequest(form))
      }}
    >
      <p className="type-meta text-muted-foreground">{t("clusterBackups.outside")}</p>
      <FormField
        label={t("clusterBackups.endpoint")}
        hint={t("clusterBackups.endpointHint")}
        errors={error("endpoint")}
      >
        <Input {...text("endpoint", "https://<account>.r2.cloudflarestorage.com")} />
      </FormField>
      {insideCluster(form.endpoint) ? <InsideWarning /> : null}
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        <FormField label={t("clusterBackups.bucket")} errors={error("bucket")}>
          <Input {...text("bucket")} />
        </FormField>
        <FormField
          label={t("clusterBackups.folder")}
          hint={t("clusterBackups.folderHint")}
          errors={error("prefix")}
        >
          <Input {...text("prefix", "zeabur")} />
        </FormField>
        <FormField
          label={t("clusterBackups.accessKey")}
          hint={editing ? t("clusterBackups.keepKeys") : undefined}
          errors={[...error("keys"), ...error("bothKeys")]}
        >
          <Input {...text("accessKeyId")} />
        </FormField>
        <FormField label={t("clusterBackups.secretKey")}>
          <Input
            size="sm"
            mono
            type="password"
            value={form.secretAccessKey}
            autoComplete="new-password"
            onChange={(event) => set({ secretAccessKey: event.target.value })}
          />
        </FormField>
        <FormField label={t("clusterBackups.region")} hint={t("clusterBackups.regionHint")}>
          <Input {...text("region", "us-east-1")} />
        </FormField>
      </div>
      <div className="flex justify-end gap-2">
        {onCancel ? (
          <Button size="sm" variant="ghost" type="button" onClick={onCancel}>
            {t("clusterBackups.cancel")}
          </Button>
        ) : null}
        <GateTip gate={gate}>
          <Button size="sm" type="submit">
            {t("edit.review")}
          </Button>
        </GateTip>
      </div>
    </form>
  )
}

function useScope() {
  const t = useT()
  return (namespaces: string[]) =>
    namespaces.length === 0 || namespaces.includes("*")
      ? t("clusterBackups.wholeCluster")
      : t("common.list", namespaces)
}

function ScheduleSection({
  data,
  gate,
  onPlan,
}: {
  data: Data
  gate: Gate
  onPlan: (request: Request, title: string) => void
}) {
  const t = useT()
  const scope = useScope()
  const schedule = mainSchedule(data)
  const others = data.schedules.filter((s) => !s.zeabur)
  const [editing, setEditing] = useState(false)
  const hasLocation = mainLocation(data) !== undefined
  return (
    <Section
      id="backup-schedule"
      title={t("clusterBackups.schedule")}
      action={
        schedule && !editing ? (
          <div className="flex items-center gap-2">
            <GateTip gate={gate}>
              <Button
                size="xs"
                variant="ghost"
                onClick={() =>
                  onPlan(
                    { op: "pause", paused: !schedule.paused },
                    t(schedule.paused ? "clusterBackups.plan.resume" : "clusterBackups.plan.pause"),
                  )
                }
              >
                {t(schedule.paused ? "clusterBackups.resume" : "clusterBackups.pause")}
              </Button>
            </GateTip>
            <GateTip gate={gate}>
              <Button size="xs" variant="outline" onClick={() => setEditing(true)}>
                {t("clusterBackups.edit")}
              </Button>
            </GateTip>
          </div>
        ) : null
      }
    >
      {schedule && !editing ? (
        <Rows>
          <Row label={t("clusterBackups.runs")}>
            <Schedule cron={schedule.cron} />
            {schedule.paused ? (
              <StatusPill health="idle" size="sm" dot={false}>
                {t("backups.paused")}
              </StatusPill>
            ) : null}
          </Row>
          <Row label={t("clusterBackups.keeps")}>
            {schedule.keepDays ? t("clusterBackups.keepDays", schedule.keepDays) : "—"}
          </Row>
          <Row label={t("clusterBackups.covers")}>
            <span className="min-w-0">{scope(schedule.namespaces)}</span>
          </Row>
          <Row label={t("clusterBackups.volumes")}>
            {t(schedule.fsBackup ? "clusterBackups.fsOn" : "clusterBackups.fsOff")}
          </Row>
        </Rows>
      ) : null}
      {schedule?.validationErrors.length ? (
        <Alert variant="warn">
          <WarningIcon aria-hidden="true" />
          <AlertDescription>{schedule.validationErrors.join(" ")}</AlertDescription>
        </Alert>
      ) : null}
      {!schedule && !editing ? (
        <p className="type-meta text-muted-foreground">
          {hasLocation ? t("clusterBackups.noSchedule") : t("clusterBackups.needLocation")}
        </p>
      ) : null}
      {editing || (!schedule && hasLocation) ? (
        <ScheduleEditor
          initial={scheduleForm(schedule)}
          current={schedule}
          gate={gate}
          onCancel={schedule ? () => setEditing(false) : undefined}
          onReview={(request) => {
            setEditing(false)
            onPlan(request, t("clusterBackups.plan.schedule"))
          }}
        />
      ) : null}
      {others.length > 0 ? (
        <ul className="flex flex-col">
          {others.map((s) => (
            <li
              key={s.name}
              className="flex flex-wrap items-center gap-x-2.5 gap-y-1 border-t border-rule py-2.5 type-meta text-muted-foreground"
            >
              <span className="type-mono text-foreground" translate="no">
                {t("clusterBackups.other", s.name)}
              </span>
              <Schedule cron={s.cron} />
              <span>{scope(s.namespaces)}</span>
              {s.paused ? (
                <StatusPill health="idle" size="sm" dot={false}>
                  {t("backups.paused")}
                </StatusPill>
              ) : null}
            </li>
          ))}
        </ul>
      ) : null}
    </Section>
  )
}

const pad = (n: number) => String(n).padStart(2, "0")

function ScheduleEditor({
  initial,
  current,
  gate,
  onCancel,
  onReview,
}: {
  initial: ScheduleForm
  current: Schemas["VeleroSchedule"] | undefined
  gate: Gate
  onCancel?: () => void
  onReview: (request: Request) => void
}) {
  const t = useT()
  const id = useId()
  const [form, setForm] = useState(initial)
  const [tried, setTried] = useState(false)
  const projects = useQuery(projectsQuery)
  const problems = scheduleProblems(form)
  const set = (patch: Partial<ScheduleForm>) => setForm((v) => ({ ...v, ...patch }))
  const error = (problem: (typeof problems)[number]) =>
    tried && problems.includes(problem) ? [t(`clusterBackups.problem.${problem}`)] : []
  const custom = current && hourOf(current.cron) === undefined ? current.cron : undefined
  const hours = [
    ...(custom ? [{ value: "custom", label: <CustomLabel cron={custom} /> }] : []),
    ...Array.from({ length: 24 }, (_, h) => ({
      value: String(h),
      label: t("clusterBackups.daily", `${pad(h)}:00`),
    })),
  ]
  const choices = [
    ...new Set([
      ...(projects.data?.items ?? []).filter((p) => p.adopted && !p.system).map((p) => p.name),
      ...form.namespaces,
    ]),
  ].sort()
  const toggle = (name: string, on: boolean) =>
    set({
      namespaces: on ? [...form.namespaces, name] : form.namespaces.filter((n) => n !== name),
    })
  return (
    <form
      className={formBox}
      onSubmit={(event) => {
        event.preventDefault()
        setTried(true)
        if (problems.length === 0) onReview(scheduleRequest(form, current))
      }}
    >
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        <FormField label={t("clusterBackups.time")}>
          <Select
            size="sm"
            className="w-full"
            value={form.hour === undefined ? "custom" : String(form.hour)}
            onValueChange={(value) => set({ hour: value === "custom" ? undefined : Number(value) })}
            items={hours}
          />
        </FormField>
        <FormField label={t("clusterBackups.keep")} errors={error("keep")}>
          <Input
            size="sm"
            type="number"
            min={1}
            max={365}
            value={String(form.keepDays)}
            onChange={(event) => set({ keepDays: Number(event.target.value) })}
          />
        </FormField>
      </div>
      <fieldset className="flex flex-col gap-2">
        <legend className="mb-2 type-body font-medium text-main">
          {t("clusterBackups.covers")}
        </legend>
        <RadioGroup
          value={form.scope}
          onValueChange={(value) => set({ scope: value as ScheduleForm["scope"] })}
        >
          <RadioOption
            value="cluster"
            label={t("clusterBackups.wholeCluster")}
            description={t("clusterBackups.wholeClusterHint")}
          />
          <RadioOption
            value="projects"
            label={t("clusterBackups.chosenProjects")}
            description={t("clusterBackups.chosenProjectsHint")}
          />
        </RadioGroup>
        {form.scope === "projects" ? (
          <>
            <ul className="max-h-56 divide-y divide-main overflow-y-auto rounded-lg border border-main">
              {choices.map((name) => (
                <li key={name} className="flex h-10 min-w-0 items-center gap-2.5 px-3">
                  <Checkbox
                    id={`${id}-${name}`}
                    checked={form.namespaces.includes(name)}
                    onCheckedChange={(on) => toggle(name, on)}
                  />
                  <Label htmlFor={`${id}-${name}`} className="min-w-0 flex-1">
                    <span className="truncate type-mono" translate="no">
                      {name}
                    </span>
                  </Label>
                </li>
              ))}
            </ul>
            {error("projects").map((message) => (
              <p key={message} className="type-meta text-destructive">
                {message}
              </p>
            ))}
          </>
        ) : null}
      </fieldset>
      <div className="flex justify-end gap-2">
        {onCancel ? (
          <Button size="sm" variant="ghost" type="button" onClick={onCancel}>
            {t("clusterBackups.cancel")}
          </Button>
        ) : null}
        <GateTip gate={gate}>
          <Button size="sm" type="submit">
            {t("edit.review")}
          </Button>
        </GateTip>
      </div>
    </form>
  )
}

function CustomLabel({ cron }: { cron: string }) {
  const t = useT()
  return <>{t("clusterBackups.asSet", cron)}</>
}

function BackupList({ data }: { data: Data }) {
  const t = useT()
  const shown = data.backups
  return (
    <Section
      id="backup-list"
      title={t("clusterBackups.backups")}
      count={data.totalBackups > 0 ? data.totalBackups : undefined}
    >
      {shown.length === 0 ? (
        <p className="type-meta text-muted-foreground">{t("clusterBackups.none")}</p>
      ) : (
        <ul className="flex flex-col">
          {shown.map((b) => (
            <BackupRow key={b.name} backup={b} />
          ))}
        </ul>
      )}
      {data.totalBackups > shown.length ? (
        <p className="type-meta text-muted-foreground">
          {t("clusterBackups.newest", shown.length, data.totalBackups)}
        </p>
      ) : null}
    </Section>
  )
}

function BackupRow({ backup }: { backup: Schemas["VeleroBackup"] }) {
  const t = useT()
  const { locale } = useI18n()
  const scope = useScope()
  const { memory } = useQuantityFormat()
  const facts = [
    scope(backup.namespaces),
    backup.items != null ? t("clusterBackups.items", backup.items, backup.totalItems) : "",
    backup.bytes ? memory(backup.bytes) : "",
    backup.errors > 0 ? t("clusterBackups.errors", backup.errors) : "",
    backup.warnings > 0 ? t("clusterBackups.warnings", backup.warnings) : "",
    backup.expires ? t("clusterBackups.keptUntil", formatDate(backup.expires, locale)) : "",
  ].filter(Boolean)
  return (
    <li className="flex flex-col gap-0.5 border-t border-rule py-2.5">
      <div className="flex min-w-0 items-center gap-2.5">
        <span className="min-w-0 flex-1 truncate type-mono" translate="no">
          {backup.name}
        </span>
        <StatusPill health={outcomeHealth[backup.outcome]} size="sm">
          {t(`backups.${backup.outcome}`)}
        </StatusPill>
        <Ago
          iso={takenAt(backup)}
          className="shrink-0 text-right type-meta text-muted-foreground sm:w-24"
        />
      </div>
      <span className="type-meta text-muted-foreground">{facts.join(t("common.sep"))}</span>
      {backup.failure ? (
        <p
          className={cn(
            "type-meta",
            backup.outcome === "failed" ? "text-fail-strong" : "text-warn-strong",
          )}
        >
          {t(`clusterBackups.failure.${backup.failure}`)}
          {backup.reason && backup.failure !== "itemsFailed" ? (
            <span className="text-muted-foreground" translate="no">
              {" "}
              {backup.reason}
            </span>
          ) : null}
        </p>
      ) : null}
    </li>
  )
}

/** What a change writes, and applying it; planning writes nothing. */
function PlanDialog({
  request,
  title,
  onClose,
}: {
  request: Request
  title: string
  onClose: () => void
}) {
  const t = useT()
  const plan = useQuery({
    queryKey: ["cluster-backups-plan", request],
    queryFn: ({ signal }) => planClusterBackups(request, signal),
    retry: false,
    staleTime: Infinity,
    gcTime: 0,
  })
  const apply = useMutation({
    mutationFn: (token: string) => applyClusterBackups(token),
    onSuccess: () => {
      refresh()
      onClose()
    },
    onError: refresh,
  })
  const left = useSecondsLeft(plan.data?.expiresAt)
  const expired = left !== undefined && left <= 0
  const writes = plan.data?.writes ?? []
  const nothing = plan.data !== undefined && writes.length === 0
  const ready = plan.data && !plan.isFetching
  const thing = (write: Schemas["BackupWrite"]) =>
    write.kind === "Backup"
      ? t("clusterBackups.thing.Backup", write.name)
      : t(`clusterBackups.thing.${write.kind as "Secret"}`)
  return (
    <Frame
      title={title}
      wide
      onClose={onClose}
      footer={
        <div className="flex w-full flex-wrap items-center gap-2">
          <Button variant="ghost" size="sm" onClick={onClose}>
            {t("plan.cancel")}
          </Button>
          {ready && left !== undefined && left < COUNTDOWN_FROM_SECONDS ? (
            <span className="type-meta text-muted-foreground tabular-nums">
              {expired ? t("plan.expired") : t("plan.expiresIn", clock(left))}
            </span>
          ) : null}
          <span className="ml-auto">
            {expired ? (
              <Button size="sm" onClick={() => void plan.refetch()}>
                {t("plan.replan")}
              </Button>
            ) : (
              <Button
                size="sm"
                disabled={!ready || nothing}
                loading={apply.isPending}
                onClick={() => plan.data && apply.mutate(plan.data.token)}
              >
                {t("plan.apply")}
              </Button>
            )}
          </span>
        </div>
      }
    >
      {plan.isPending || plan.isFetching ? (
        <LoadingRows label={t("plan.planning")} rows={3} />
      ) : plan.isError ? (
        <Problem error={plan.error} />
      ) : nothing ? (
        <p>{t("clusterBackups.plan.nothing")}</p>
      ) : (
        <>
          <ul className="flex flex-col gap-1">
            {writes.map((w) => (
              <li key={`${w.kind}/${w.name}`}>{t(`clusterBackups.plan.${w.action}`, thing(w))}</li>
            ))}
          </ul>
          {writes.some((w) => w.kind === "Secret") ? (
            <p className="type-meta text-muted-foreground">{t("clusterBackups.plan.keysHidden")}</p>
          ) : null}
          <Writes writes={writes} />
        </>
      )}
      {apply.isError ? <Problem error={apply.error} /> : null}
    </Frame>
  )
}

function Writes({ writes }: { writes: Schemas["BackupWrite"][] }) {
  const t = useT()
  return (
    <Collapsible>
      <CollapsibleTrigger variant="disclosure" className="mt-1">
        <CaretRightIcon aria-hidden="true" />
        {t("plan.details")}
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="flex flex-col gap-4 pt-3">
          {writes.map((write) => (
            <section
              key={`${write.kind}/${write.name}`}
              aria-label={`${write.kind}/${write.name}`}
              className="flex min-w-0 flex-col gap-2"
            >
              <span className="min-w-0 font-mono text-sm font-medium break-all" translate="no">
                {`${write.kind}/${write.name}`}
                <span className="ml-2 font-sans font-normal type-meta text-muted-foreground">
                  {write.namespace}
                </span>
              </span>
              <YamlDiff before={write.beforeYaml ?? ""} after={write.afterYaml ?? ""} />
            </section>
          ))}
        </div>
      </CollapsibleContent>
    </Collapsible>
  )
}
