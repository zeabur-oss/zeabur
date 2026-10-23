// Notifications: where a project's alerts go, or the cluster's. Each channel is a
// VMAlertmanagerConfig and a Secret; every change is a plan.
import { useMutation, useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { useId, useState } from "react"
import { toast } from "sonner"
import DotsThreeIcon from "~icons/ph/dots-three"
import WarningIcon from "~icons/ph/warning"

import { isStatus, type Schemas } from "@/api/client"
import {
  applyNotifications,
  notificationsQuery,
  planNotifications,
  queryClient,
} from "@/api/queries"
import { PlanDiffs, PlanFrame } from "@/components/cluster-plan"
import { FormField } from "@/components/form-field"
import { GateTip, useHintGate } from "@/components/gate"
import { KubeDetails } from "@/components/kube-details"
import { Frame } from "@/components/members/frame"
import { LoadingRows } from "@/components/page"
import { Problem } from "@/components/problem"
import { permission, Refused } from "@/components/refused"
import { StatusDot, StatusPill } from "@/components/status"
import { useT } from "@/i18n"
import {
  FIELDS,
  fixOf,
  GOTIFY_COMPONENT,
  KINDS,
  METRICS_COMPONENT,
  missing,
  NAME_MAX,
  nameProblem,
  SECRET_FIELDS,
  settingsOf,
  statusHealth,
  verdict,
  type ChannelForm,
  type ChannelKind,
  type Event,
  type Field,
} from "@/model/notifications"
import { Alert, AlertDescription } from "@zeabur/design-system/components/alert"
import { Button, buttonVariants } from "@zeabur/design-system/components/button"
import { Card, CardRow } from "@zeabur/design-system/components/card"
import { Checkbox } from "@zeabur/design-system/components/checkbox"
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
import { Menu, MenuContent, MenuItem, MenuTrigger } from "@zeabur/design-system/components/menu"
import { Select } from "@zeabur/design-system/components/select"

type Data = Schemas["Notifications"]
type Channel = Schemas["AlertmanagerChannel"]
type Request = Schemas["NotificationsPlanRequest"]

const GROUP = "operator.victoriametrics.com"
const verdictHealth = { none: "idle", rejected: "fail", waiting: "idle", ok: "ok" } as const

/** A project's channels, or the cluster's when `project` is absent. */
export function Notifications({ project }: { project?: string }) {
  const t = useT()
  const query = useQuery(notificationsQuery(project))
  const data = query.data
  if (query.isPending) return <LoadingRows label={t("notify.title")} rows={2} />
  if (data?.state === "refused" || (query.isError && isStatus(query.error, 403)))
    return (
      <Refused
        refusal="unreadable"
        what={t("notify.what")}
        missing={[permission("list", "vmalertmanagerconfigs", GROUP)]}
        line
      />
    )
  if (query.isError) return <Problem error={query.error} />
  if (data?.state === "not-installed") return <NotInstalled />
  return data ? <Channels data={data} project={project} /> : null
}

function NotInstalled() {
  const t = useT()
  return (
    <Empty variant="outline">
      <EmptyHeader>
        <EmptyMedia>
          <Illustration kind="empty" seed="notifications" />
        </EmptyMedia>
        <EmptyTitle>{t("notify.notInstalled")}</EmptyTitle>
        <EmptyDescription>{t("notify.notInstalledBody")}</EmptyDescription>
      </EmptyHeader>
      <EmptyContent>
        <Link
          to="/cluster/components"
          search={{ component: METRICS_COMPONENT }}
          className={buttonVariants({ size: "sm" })}
        >
          {t("notify.install")}
        </Link>
      </EmptyContent>
    </Empty>
  )
}

type Open =
  | { as: "add" }
  | { as: "events"; channel: Channel }
  | { as: "plan"; request: Request; title: string }

function Channels({ data, project }: { data: Data; project?: string }) {
  const t = useT()
  const change = useHintGate(
    project ? "changeNotifications" : "changeClusterNotifications",
    data.canChange,
  )
  const test = useHintGate(project ? "testNotifications" : "testClusterNotifications", data.canTest)
  const [open, setOpen] = useState<Open>()
  const close = () => setOpen(undefined)
  const plan = (request: Request, title: string) => setOpen({ as: "plan", request, title })
  const v = verdict(data.channels)
  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
        <p
          className="flex min-w-0 flex-1 items-center gap-2 type-body font-medium text-main"
          role="status"
        >
          <StatusDot health={verdictHealth[v.as]} />
          {v.as === "none" ? t("notify.verdict.none") : t(`notify.verdict.${v.as}`, v.count)}
        </p>
        <GateTip gate={change}>
          <Button size="sm" variant="outline" onClick={() => setOpen({ as: "add" })}>
            {t("notify.add")}
          </Button>
        </GateTip>
      </div>
      {data.rulesInstalled === false ? (
        <Alert variant="warn" role="note">
          <WarningIcon aria-hidden="true" />
          <AlertDescription>{t("notify.noRules")}</AlertDescription>
        </Alert>
      ) : null}
      {data.channels.length > 0 ? (
        <Card variant="list">
          {data.channels.map((channel) => (
            <ChannelRow
              key={channel.config}
              channel={channel}
              offered={data.events}
              change={change}
              test={test}
              onTest={() =>
                plan({ op: "test", name: channel.name }, t("notify.plan.test", channel.name))
              }
              onEvents={() => setOpen({ as: "events", channel })}
              onRemove={() =>
                plan({ op: "remove", name: channel.name }, t("notify.plan.remove", channel.name))
              }
            />
          ))}
        </Card>
      ) : null}
      <KubeDetails facts={kubeFacts(t, data)} />
      {open ? (
        <OpenDialog open={open} data={data} project={project} onClose={close} onPlan={plan} />
      ) : null}
    </div>
  )
}

/** What to do, when Zeabur knows the reason; the operator's words when not. */
function Reason({ reason }: { reason: string }) {
  const t = useT()
  const fix = fixOf(reason)
  return fix ? (
    <span className="type-meta text-fail-strong">{t(`notify.fix.${fix}`)}</span>
  ) : (
    <span className="type-meta break-words text-fail-strong" translate="no">
      {reason}
    </span>
  )
}

function kubeFacts(t: ReturnType<typeof useT>, data: Data): [string, string][] {
  const managed = data.channels.filter((c) => c.managed)
  return [
    [t("notify.kube.namespace"), data.namespace],
    ...managed.map((c): [string, string] => [
      t("notify.kube.channel", c.name),
      `VMAlertmanagerConfig/${c.config}`,
    ]),
    ...data.channels.flatMap((c): [string, string][] =>
      c.reason ? [[t("notify.kube.reason", c.name), c.reason]] : [],
    ),
    ...(managed.length > 0
      ? ([[t("notify.kube.secrets"), t("notify.kube.secretsValue")]] as [string, string][])
      : []),
    [t("notify.kube.test"), t("notify.kube.testValue")],
  ]
}

function OpenDialog({
  open,
  data,
  project,
  onClose,
  onPlan,
}: {
  open: Open
  data: Data
  project?: string
  onClose: () => void
  onPlan: (request: Request, title: string) => void
}) {
  const t = useT()
  switch (open.as) {
    case "add":
      return (
        <ChannelDialog
          events={data.events}
          onClose={onClose}
          onReview={(form) =>
            onPlan(
              { op: "add", name: form.name, events: form.events, settings: settingsOf(form) },
              t("notify.plan.add", form.name),
            )
          }
        />
      )
    case "events":
      return (
        <EventsDialog
          channel={open.channel}
          events={data.events}
          onClose={onClose}
          onReview={(events) =>
            onPlan(
              { op: "events", name: open.channel.name, events },
              t("notify.plan.events", open.channel.name),
            )
          }
        />
      )
    case "plan":
      return (
        <PlanDialog project={project} request={open.request} title={open.title} onClose={onClose} />
      )
  }
}

function ChannelRow({
  channel,
  offered,
  change,
  test,
  onTest,
  onEvents,
  onRemove,
}: {
  channel: Channel
  offered: Event[]
  change: ReturnType<typeof useHintGate>
  test: ReturnType<typeof useHintGate>
  onTest: () => void
  onEvents: () => void
  onRemove: () => void
}) {
  const t = useT()
  const where = [t(`notify.kind.${channel.kind}`), channel.target]
    .filter(Boolean)
    .join(t("common.sep"))
  const events = offered
    .filter((e) => channel.events.includes(e))
    .map((e) => t(`notify.event.${e}`))
    .join(t("common.sep"))
  return (
    <CardRow
      label={
        <span className="flex min-w-0 flex-wrap items-center gap-2">
          <span className="min-w-0 truncate type-mono" translate="no">
            {channel.name}
          </span>
          {channel.status === "active" ? null : (
            <StatusPill health={statusHealth[channel.status]} size="sm">
              {t(`notify.status.${channel.status}`)}
            </StatusPill>
          )}
        </span>
      }
      description={
        <span className="flex flex-col gap-0.5">
          <span className="break-words">{where}</span>
          <span className="type-meta text-muted-foreground">
            {channel.managed ? events : t("notify.unmanaged")}
          </span>
          {channel.reason ? <Reason reason={channel.reason} /> : null}
        </span>
      }
    >
      {channel.managed ? (
        <span className="flex items-center gap-1">
          <GateTip gate={test}>
            <Button size="sm" variant="outline" onClick={onTest}>
              {t("notify.test")}
            </Button>
          </GateTip>
          <Menu>
            <GateTip gate={change}>
              <MenuTrigger
                render={
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    aria-label={t("notify.more", channel.name)}
                  />
                }
              >
                <DotsThreeIcon aria-hidden="true" />
              </MenuTrigger>
            </GateTip>
            <MenuContent align="end">
              <MenuItem onClick={onEvents}>{t("notify.chooseEvents")}</MenuItem>
              <MenuItem onClick={onRemove}>{t("notify.remove")}</MenuItem>
            </MenuContent>
          </Menu>
        </span>
      ) : null}
    </CardRow>
  )
}

function EventChoices({
  events,
  chosen,
  onChange,
  error,
}: {
  events: Event[]
  chosen: Event[]
  onChange: (events: Event[]) => void
  error?: string
}) {
  const t = useT()
  const id = useId()
  return (
    <fieldset className="flex flex-col gap-2">
      <legend className="mb-2 type-body font-medium text-main">{t("notify.events")}</legend>
      <ul className="grid grid-cols-1 gap-x-4 gap-y-1 sm:grid-cols-2">
        {events.map((event) => (
          <li key={event} className="flex min-h-8 min-w-0 items-center gap-2.5">
            <Checkbox
              id={`${id}-${event}`}
              checked={chosen.includes(event)}
              onCheckedChange={(on) =>
                onChange(on ? [...chosen, event] : chosen.filter((e) => e !== event))
              }
            />
            <Label htmlFor={`${id}-${event}`} className="min-w-0 flex-1">
              {t(`notify.event.${event}`)}
            </Label>
          </li>
        ))}
      </ul>
      {error ? <p className="type-meta text-destructive">{error}</p> : null}
    </fieldset>
  )
}

function ChannelDialog({
  events,
  onClose,
  onReview,
}: {
  events: Event[]
  onClose: () => void
  onReview: (form: ChannelForm) => void
}) {
  const t = useT()
  const formId = useId()
  const [form, setForm] = useState<ChannelForm>({
    name: "",
    kind: "slack",
    values: {},
    events: events.filter((e) => e.endsWith("-failed") || e === "crash-loop"),
  })
  const [tried, setTried] = useState(false)
  const needed = missing(form)
  const badName = nameProblem(form.name)
  const ok = !badName && needed.length === 0 && form.events.length > 0
  const set = (field: Field, value: string) =>
    setForm((f) => ({ ...f, values: { ...f.values, [field]: value } }))
  return (
    <Frame
      title={t("notify.addTitle")}
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" size="sm" onClick={onClose}>
            {t("plan.cancel")}
          </Button>
          <Button size="sm" type="submit" form={formId}>
            {t("edit.review")}
          </Button>
        </>
      }
    >
      <form
        id={formId}
        className="flex flex-col gap-4"
        onSubmit={(event) => {
          event.preventDefault()
          setTried(true)
          if (ok) onReview(form)
        }}
      >
        <FormField
          label={t("notify.name")}
          hint={t("notify.nameHint", NAME_MAX)}
          errors={tried && badName ? [t("notify.nameProblem")] : []}
        >
          <Input
            size="sm"
            value={form.name}
            autoComplete="off"
            spellCheck={false}
            maxLength={NAME_MAX}
            placeholder="team-alerts"
            onChange={(event) => setForm((f) => ({ ...f, name: event.target.value }))}
          />
        </FormField>
        <FormField label={t("notify.sendTo")}>
          <Select
            size="sm"
            className="w-full"
            value={form.kind}
            onValueChange={(kind) =>
              setForm((f) => ({ ...f, kind: kind as ChannelKind, values: {} }))
            }
            items={KINDS.map((kind) => ({ value: kind, label: t(`notify.kind.${kind}`) }))}
          />
        </FormField>
        {FIELDS[form.kind].map(([field, optional]) => (
          <FormField
            key={`${form.kind}-${field}`}
            label={
              optional
                ? t("notify.optional", t(`notify.field.${field}`))
                : t(`notify.field.${field}`)
            }
            hint={hintOf(form.kind, field, t)}
            errors={
              tried && needed.includes(field)
                ? [t(field === "chatId" ? "notify.chatIdProblem" : "notify.required")]
                : []
            }
          >
            <Input
              size="sm"
              type={SECRET_FIELDS.includes(field) ? "password" : "text"}
              inputMode={field === "chatId" ? "numeric" : undefined}
              autoComplete="off"
              spellCheck={false}
              value={form.values[field] ?? ""}
              onChange={(event) => set(field, event.target.value)}
            />
          </FormField>
        ))}
        {form.kind === "gotify" ? (
          <p className="type-meta text-muted-foreground">
            {t("notify.gotifyBridge")}{" "}
            <Link
              to="/cluster/components"
              search={{ component: GOTIFY_COMPONENT }}
              className="text-foreground hover:underline"
            >
              {t("notify.gotifyInstall")}
            </Link>
          </p>
        ) : null}
        <EventChoices
          events={events}
          chosen={form.events}
          onChange={(chosen) => setForm((f) => ({ ...f, events: chosen }))}
          error={tried && form.events.length === 0 ? t("notify.noEvents") : undefined}
        />
        <p className="type-meta text-muted-foreground">{t("notify.secretNote")}</p>
      </form>
    </Frame>
  )
}

function hintOf(kind: ChannelKind, field: Field, t: ReturnType<typeof useT>) {
  if (field === "webhookUrl") return t(`notify.hint.${kind as "slack"}`)
  if (field === "server") return t("notify.hint.ntfyServer")
  if (field === "smarthost") return t("notify.hint.smarthost")
  if (field === "chatId") return t("notify.hint.chatId")
  return undefined
}

function EventsDialog({
  channel,
  events,
  onClose,
  onReview,
}: {
  channel: Channel
  events: Event[]
  onClose: () => void
  onReview: (events: Event[]) => void
}) {
  const t = useT()
  const [chosen, setChosen] = useState(channel.events)
  return (
    <Frame
      title={t("notify.plan.events", channel.name)}
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" size="sm" onClick={onClose}>
            {t("plan.cancel")}
          </Button>
          <Button size="sm" disabled={chosen.length === 0} onClick={() => onReview(chosen)}>
            {t("edit.review")}
          </Button>
        </>
      }
    >
      <EventChoices
        events={events}
        chosen={chosen}
        onChange={setChosen}
        error={chosen.length === 0 ? t("notify.noEvents") : undefined}
      />
    </Frame>
  )
}

/** What a change writes, and applying it; planning writes nothing. */
function PlanDialog({
  project,
  request,
  title,
  onClose,
}: {
  project?: string
  request: Request
  title: string
  onClose: () => void
}) {
  const t = useT()
  const key = notificationsQuery(project).queryKey
  const refresh = () => void queryClient.invalidateQueries({ queryKey: key })
  const plan = useQuery({
    queryKey: [...key, "plan", request],
    queryFn: ({ signal }) => planNotifications(project, request, signal),
    retry: false,
    staleTime: Infinity,
    gcTime: 0,
  })
  const apply = useMutation({
    mutationFn: (token: string) => applyNotifications(project, token),
    onSuccess: () => {
      if (request.op === "test") toast.success(t("notify.testSent", request.name))
      refresh()
      onClose()
    },
    onError: refresh,
  })
  return (
    <PlanFrame
      title={title}
      plan={plan}
      apply={apply}
      action={request.op === "test" ? t("notify.test") : t("plan.apply")}
      ready={(data) => (data?.writes.length ?? 0) > 0}
      onClose={onClose}
    >
      {(data) => <Planned request={request} writes={data.writes} />}
    </PlanFrame>
  )
}

function Planned({
  request,
  writes,
}: {
  request: Request
  writes: Schemas["NotificationWrite"][]
}) {
  const t = useT()
  if (writes.length === 0) return <p>{t("notify.plan.nothing")}</p>
  const thing = (write: Schemas["NotificationWrite"]) =>
    write.kind === "Secret"
      ? t("notify.thing.secret")
      : write.kind === "Job"
        ? t("notify.thing.job")
        : t("notify.thing.channel", request.name)
  return (
    <>
      {request.op === "test" ? <p>{t("notify.plan.testHow")}</p> : null}
      <ul className="flex flex-col gap-1">
        {writes.map((write) => (
          <li key={`${write.kind}/${write.name}`}>
            {t(`notify.plan.${write.action}`, thing(write))}
          </li>
        ))}
      </ul>
      {writes.some((write) => write.kind === "Secret" && write.action === "create") ? (
        <p className="type-meta text-muted-foreground">{t("notify.plan.hidden")}</p>
      ) : null}
      <PlanDiffs writes={writes} />
    </>
  )
}
