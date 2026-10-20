import { useQuery } from "@tanstack/react-query"
import { getRouteApi, Link } from "@tanstack/react-router"
import { lazy, Suspense, type ReactNode } from "react"

import { historyQuery, usageQuery } from "@/api/queries"
import { Hint, Panel, Segmented } from "@/components/bits"
import { Refused } from "@/components/refused"
import { Skeleton } from "@zeabur/design-system/components/skeleton"
import { Tooltip } from "@zeabur/design-system/components/tooltip"
import { useT } from "@/i18n"
import { useQuantityFormat as useFormat } from "@/lib/quantity"
import { cn } from "@/lib/utils"
import type { Service } from "@/model/service"
import {
  colours,
  hasValues,
  measured,
  NEAR_LIMIT,
  notesFor,
  podsAtEnd,
  ranges,
  replaced,
  sumUsage,
  templateAmounts,
  total,
  type Amount,
  type Measured,
  type Note,
  type ServiceHistory,
} from "@/model/usage"
import type { Guide } from "@/routes/service/history-chart"
import { Meter } from "@zeabur/design-system/components/meter"
import { SummaryCardAside } from "@zeabur/design-system/components/summary-card"

const HistoryChart = lazy(() => import("@/routes/service/history-chart"))

const route = getRouteApi("/projects/$project/services/$kind/$name")

export function UsagePanel({ project, service }: { project: string; service: Service }) {
  const t = useT()
  const usage = useQuery(usageQuery(project))
  const state = usage.data?.state
  const { requests, limits } = templateAmounts(service)
  const available = state === "available"
  const rows = measured(service, available ? usage.data : undefined)
  const sum = sumUsage(rows)
  const replicas = service.pods.length
  // The totals compare what was measured with what those replicas reserve: one without a sample
  // adds neither.
  const counted = available && sum.pods > 0 ? sum.pods : replicas
  const scale = (value?: number) => (value === undefined ? undefined : value * counted)
  const noteRows = rows.map((row) => ({ row, notes: notesFor(row, requests, limits) }))
  const nearLimit = available
    ? noteRows.filter((r) => r.notes.some((n) => n.kind === "memory-near-limit"))
    : []
  return (
    <Panel
      id="usage"
      title={t("usage.title")}
      className="lg:col-span-2"
      bodyClassName="p-0"
      action={
        available ? (
          <>
            <span className="flex items-center gap-1.5 type-meta text-muted-foreground">
              <span className="size-1.5 rounded-full bg-ok" aria-hidden="true" />
              {t("usage.live")}
            </span>
            <Hint label={t("usage.title")}>{t("usage.liveHint")}</Hint>
          </>
        ) : (
          <Hint label={t("usage.title")}>{t("resources.hint")}</Hint>
        )
      }
    >
      <div className="flex flex-col" data-usage={state ?? "loading"}>
        {nearLimit.length > 0 ? (
          <p className="flex items-center gap-2 border-b bg-warn-soft px-5 py-2 text-sm text-warn-strong">
            <span className="size-1.5 shrink-0 rounded-full bg-warn" aria-hidden="true" />
            {nearLimit.length === 1
              ? t("usage.nearLimitOne", nearLimit[0]?.row.pod.name ?? "")
              : t("usage.nearLimitMany", nearLimit.length)}
          </p>
        ) : null}
        <div className="grid grid-cols-1 gap-x-8 gap-y-5 px-5 py-4 md:grid-cols-2">
          <TotalBlock
            title={t("resources.cpu")}
            used={available ? sum.cpu : undefined}
            request={scale(requests.cpu)}
            limit={scale(limits.cpu)}
            kind="cpu"
            replicas={replicas}
            measuredPods={sum.pods}
            showUsage={available}
          />
          <TotalBlock
            title={t("resources.memory")}
            used={available ? sum.memory : undefined}
            request={scale(requests.memory)}
            limit={scale(limits.memory)}
            kind="memory"
            replicas={replicas}
            measuredPods={sum.pods}
            showUsage={available}
          />
        </div>
        {state === "not-installed" ? (
          <p className="flex flex-wrap items-center gap-x-3 gap-y-1 border-t px-5 py-3 text-sm text-muted-foreground">
            {t("usage.needsMetricsServer")}
            <Link
              to="/cluster/components"
              search={{ component: "metrics/metrics-server" }}
              className="font-medium text-foreground hover:underline"
            >
              {t("usage.install")}
            </Link>
          </p>
        ) : null}
        {state === "unreadable" ? (
          <div className="border-t px-5 py-3">
            <Refused
              refusal="unreadable"
              what={t("usage.whatLive")}
              missing={["list pods.metrics.k8s.io"]}
              line
            />
          </div>
        ) : null}
        {state === "unavailable" ? (
          <p className="border-t px-5 py-3 text-sm text-muted-foreground">
            {t("usage.unavailable")}
          </p>
        ) : null}
        {available ? (
          <ReplicaTable rows={noteRows} requests={requests} limits={limits} />
        ) : (
          <TemplateLine requests={requests} limits={limits} replicas={replicas} />
        )}
      </div>
    </Panel>
  )
}

/** Live CPU and memory beside the service's status; nothing until metrics-server answers. */
export function UsageGlance({
  project,
  service,
  action,
}: {
  project: string
  service: Service
  action: ReactNode
}) {
  const t = useT()
  const usage = useQuery(usageQuery(project))
  if (usage.data?.state !== "available") return null
  const { requests, limits } = templateAmounts(service)
  const sum = sumUsage(measured(service, usage.data))
  if (sum.pods === 0) return null
  const scale = (value?: number) => (value === undefined ? undefined : value * sum.pods)
  const replicas = service.pods.length
  return (
    <SummaryCardAside>
      <div className="flex items-center justify-between gap-2">
        <span className="flex items-center gap-1.5 type-meta text-muted-foreground">
          <span className="size-1.5 rounded-full bg-ok" aria-hidden="true" />
          {t("usage.title")}
        </span>
        {action}
      </div>
      {(["cpu", "memory"] as const).map((kind) => (
        <TotalBlock
          key={kind}
          title={t(kind === "cpu" ? "resources.cpu" : "resources.memory")}
          used={sum[kind]}
          request={scale(requests[kind])}
          limit={scale(limits[kind])}
          kind={kind}
          replicas={replicas}
          measuredPods={sum.pods}
          showUsage
        />
      ))}
    </SummaryCardAside>
  )
}

function UsageBar({
  used,
  request,
  limit,
  label,
  size = "md",
}: {
  used?: number
  request?: number
  limit?: number
  label: string
  size?: "sm" | "md"
}) {
  const room = limit ?? Math.max(request ?? 0, used ?? 0) * 1.25
  const share = used !== undefined && room > 0 ? used / room : undefined
  const tone =
    limit && share !== undefined && share >= NEAR_LIMIT
      ? "warn"
      : request && used !== undefined && used > request
        ? "over"
        : "ok"
  return (
    <Meter
      value={share}
      mark={request !== undefined && room > 0 ? request / room : undefined}
      tone={tone}
      size={size}
      unbounded={!limit}
      label={label}
    />
  )
}

function TotalBlock({
  title,
  used,
  request,
  limit,
  kind,
  replicas,
  measuredPods,
  showUsage,
}: {
  title: string
  used?: number
  request?: number
  limit?: number
  kind: "cpu" | "memory"
  replicas: number
  measuredPods: number
  showUsage: boolean
}) {
  const t = useT()
  const f = useFormat()
  const fmt = kind === "cpu" ? f.cpu : f.memory
  const share = used !== undefined && limit ? used / limit : undefined
  const label = [
    title,
    used !== undefined ? t("usage.usedWord", fmt(used)) : "",
    request !== undefined ? t("usage.reservedWord", fmt(request)) : "",
    limit !== undefined ? t("usage.maxWord", fmt(limit)) : t("usage.noMax"),
  ]
    .filter(Boolean)
    .join(t("common.sep"))
  return (
    <div className="flex min-w-0 flex-col gap-2">
      <div className="flex items-baseline justify-between gap-2">
        <span className="type-meta text-muted-foreground">
          {title}
          {replicas > 1 ? ` ${t("usage.acrossPods", replicas)}` : ""}
        </span>
        {showUsage && used !== undefined ? (
          <span className="flex items-baseline gap-1.5">
            <span className="type-figure tabular-nums">{fmt(used)}</span>
            {share !== undefined ? (
              <span className="type-meta text-muted-foreground tabular-nums">
                {t("usage.ofMax", f.percent(share))}
              </span>
            ) : null}
          </span>
        ) : null}
      </div>
      <UsageBar used={showUsage ? used : undefined} request={request} limit={limit} label={label} />
      <span className="flex flex-wrap gap-x-3 type-meta text-muted-foreground tabular-nums">
        <span>
          {request !== undefined ? t("usage.reservedWord", fmt(request)) : t("usage.noReserved")}
        </span>
        <span>{limit !== undefined ? t("usage.maxWord", fmt(limit)) : t("usage.noMax")}</span>
        {showUsage && measuredPods < replicas ? (
          <span>{t("usage.sampledOf", measuredPods, replicas)}</span>
        ) : null}
      </span>
    </div>
  )
}

function TemplateLine({
  requests,
  limits,
  replicas,
}: {
  requests: Amount
  limits: Amount
  replicas: number
}) {
  const t = useT()
  const f = useFormat()
  const cell = (value: number | undefined, fmt: (v: number) => string) =>
    value !== undefined ? fmt(value) : "—"
  return (
    <p className="border-t px-5 py-2 type-meta text-muted-foreground tabular-nums">
      {t(
        "usage.perPodTemplate",
        replicas,
        `${cell(requests.cpu, f.millicores)} / ${cell(limits.cpu, f.millicores)}`,
        `${cell(requests.memory, f.memory)} / ${cell(limits.memory, f.memory)}`,
      )}
    </p>
  )
}

function ReplicaTable({
  rows,
  requests,
  limits,
}: {
  rows: { row: Measured; notes: Note[] }[]
  requests: Amount
  limits: Amount
}) {
  const t = useT()
  const f = useFormat()
  const pair = (
    request: number | undefined,
    limit: number | undefined,
    fmt: (v: number) => string,
  ) => `${request !== undefined ? fmt(request) : "—"} / ${limit !== undefined ? fmt(limit) : "—"}`
  return (
    <table className="w-full border-t text-xs">
      <thead>
        <tr className="border-b text-left text-muted-foreground">
          <th className="px-4 py-2 font-medium">{t("usage.replica")}</th>
          <th className="w-1/4 px-2 py-2 font-medium whitespace-nowrap">
            {t("resources.cpu")}
            <span className="ml-1.5 font-normal tabular-nums">
              {pair(requests.cpu, limits.cpu, f.millicores)}
            </span>
          </th>
          <th className="w-1/4 px-2 py-2 font-medium whitespace-nowrap">
            {t("resources.memory")}
            <span className="ml-1.5 font-normal tabular-nums">
              {pair(requests.memory, limits.memory, f.memory)}
            </span>
          </th>
          <th className="px-2 py-2 pr-5 font-medium">
            <span className="sr-only">{t("usage.notes")}</span>
          </th>
        </tr>
      </thead>
      <tbody className="divide-y">
        {rows.map(({ row, notes }) => (
          <tr key={row.pod.name}>
            <td className="w-1/3 max-w-0 truncate px-5 py-2 font-mono" translate="no">
              {row.pod.name}
            </td>
            <td className="px-2 py-2">
              <Cell
                value={row.cpu}
                text={row.cpu !== undefined ? f.millicores(row.cpu) : undefined}
                request={requests.cpu}
                limit={limits.cpu}
                label={`${t("resources.cpu")} ${row.pod.name}`}
              />
            </td>
            <td className="px-2 py-2">
              <Cell
                value={row.memory}
                text={row.memory !== undefined ? f.memory(row.memory) : undefined}
                request={requests.memory}
                limit={limits.memory}
                label={`${t("resources.memory")} ${row.pod.name}`}
              />
            </td>
            <td className="px-2 py-2 pr-5">
              <span className="flex flex-wrap justify-end gap-1">
                {notes.map((note) => (
                  <NotePill key={note.kind} note={note} />
                ))}
              </span>
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}

function Cell({
  value,
  text,
  request,
  limit,
  label,
}: {
  value?: number
  text?: string
  request?: number
  limit?: number
  label: string
}) {
  return (
    <span className="flex items-center gap-2">
      <span className="w-16 shrink-0 text-right tabular-nums">
        {text ?? <span className="text-muted-foreground">—</span>}
      </span>
      <UsageBar used={value} request={request} limit={limit} label={label} size="sm" />
    </span>
  )
}

function NotePill({ note }: { note: Note }) {
  const t = useT()
  const f = useFormat()
  const pill = (tone: "warn" | "fail" | "plain", word: string, hint: ReactNode) => (
    <Tooltip content={hint}>
      <span
        tabIndex={0}
        className={cn(
          "rounded px-1.5 py-px font-medium whitespace-nowrap outline-none",
          tone === "warn" && "bg-warn-soft text-warn-strong",
          tone === "fail" && "bg-fail-soft text-fail-strong",
          tone === "plain" && "bg-muted text-muted-foreground",
        )}
      >
        {word}
      </span>
    </Tooltip>
  )
  switch (note.kind) {
    case "oom":
      return pill("fail", t("usage.note.oom"), t("usage.note.oomHint"))
    case "memory-near-limit":
      return pill(
        "warn",
        t("usage.note.nearLimit"),
        t("usage.note.nearLimitHint", f.percent(note.share)),
      )
    case "cpu-at-limit":
      return pill("plain", t("usage.note.cpuAtLimit"), t("usage.note.cpuAtLimitHint"))
    case "memory-over-request":
      return pill("plain", t("usage.note.overRequest"), t("usage.note.overRequestHint"))
    case "no-sample":
      return pill("plain", t("usage.note.noSample"), t("usage.note.noSampleHint"))
  }
}

export function HistoryPanel({ project, service }: { project: string; service: Service }) {
  const t = useT()
  const search = route.useSearch()
  const navigate = route.useNavigate()
  const range = search.range ?? "1h"
  const split = search.split ?? "replica"
  const history = useQuery(historyQuery(project, service.kind, service.name, range))
  const data = history.data
  const set = (patch: Partial<typeof search>) =>
    void navigate({ search: (previous) => ({ ...previous, ...patch }), replace: true })
  const showsControls = data?.state === "available"
  return (
    <Panel
      id="usage-history"
      title={t("usage.history")}
      className="lg:col-span-2"
      action={
        showsControls ? (
          <>
            <Segmented
              label={t("usage.split")}
              value={split}
              onChange={(value) => set({ split: value === "replica" ? undefined : value })}
              options={[
                { value: "replica", label: t("usage.perReplica") },
                { value: "total", label: t("usage.total") },
              ]}
            />
            <Segmented
              label={t("usage.range")}
              value={range}
              onChange={(value) => set({ range: value === "1h" ? undefined : value })}
              options={ranges.map((r) => ({ value: r, label: t(`usage.range.${r}`) }))}
            />
          </>
        ) : null
      }
    >
      <div data-history={data?.state ?? (history.isError ? "error" : "loading")}>
        {data ? (
          <HistoryBody service={service} history={data} split={split} />
        ) : history.isError ? (
          <p className="text-sm text-muted-foreground">{t("usage.storeUnavailable")}</p>
        ) : (
          <Skeleton className="h-44 w-full" />
        )}
      </div>
    </Panel>
  )
}

function HistoryBody({
  service,
  history,
  split,
}: {
  service: Service
  history: ServiceHistory
  split: "replica" | "total"
}) {
  const t = useT()
  const f = useFormat()
  const range = history.range
  const store = history.source.service
  switch (history.state) {
    case "not-configured":
      return (
        <p className="flex flex-wrap items-center gap-x-3 gap-y-1 text-sm text-muted-foreground">
          {t("usage.needsStore")}
          <Link
            to="/cluster/components"
            search={{ component: "metrics/victoria-metrics" }}
            className="font-medium text-foreground hover:underline"
          >
            {t("usage.installVm")}
          </Link>
          <Link to="/settings" className="font-medium text-foreground hover:underline">
            {t("usage.chooseStore")}
          </Link>
          <Hint label={t("usage.history")}>{t("usage.needsStoreHint", store)}</Hint>
        </p>
      )
    case "unreadable":
      return (
        <Refused
          refusal="unreadable"
          what={t("usage.whatHistory")}
          missing={history.unreadable.map((u) => `get ${u.resource}`)}
          line
        >
          <p className="type-meta text-muted-foreground">{t("usage.proxyHint")}</p>
        </Refused>
      )
    case "unavailable":
      return (
        <p className="flex flex-wrap items-center gap-2 text-sm text-muted-foreground">
          {t("usage.storeUnavailable")}
          {history.message ? (
            <Hint label={t("usage.history")}>
              <span className="font-mono" translate="no">
                {store}: {history.message}
              </span>
            </Hint>
          ) : null}
        </p>
      )
    case "available":
      break
  }
  if (!hasValues(history.memory) && !hasValues(history.cpu)) {
    return (
      <p className="flex flex-wrap items-center gap-2 text-sm text-muted-foreground">
        {t("usage.noSeries", service.name, t(`usage.rangeLong.${range}`))}
        <Hint label={t("usage.history")}>{t("usage.noSeriesHint")}</Hint>
      </p>
    )
  }
  const { requests, limits } = templateAmounts(service)
  const gone = replaced(history, service)
  const pods = podsAtEnd(history.memory)
  const guides = (kind: "cpu" | "memory"): Guide[] => {
    const many = split === "total" ? Math.max(pods, 1) : 1
    const request = kind === "cpu" ? requests.cpu : requests.memory
    const limit = kind === "cpu" ? limits.cpu : limits.memory
    return [
      ...(request !== undefined ? [{ kind: "reserved" as const, value: request * many }] : []),
      ...(limit !== undefined ? [{ kind: "max" as const, value: limit * many }] : []),
    ]
  }
  const series = (kind: "cpu" | "memory") => {
    const all = kind === "cpu" ? history.cpu : history.memory
    return split === "total" ? [total(all)] : all
  }
  const restarts = history.restarts ?? []
  const span = history.step * Math.max(history.times.length - 1, 1)
  const stepText =
    history.step < 60
      ? t("usage.stepSeconds", history.step)
      : t("usage.stepMinutes", history.step / 60)
  const chart = (kind: "cpu" | "memory") => (
    <div className="flex min-w-0 flex-col gap-2">
      <span className="type-meta font-medium">
        {kind === "cpu" ? t("resources.cpu") : t("resources.memory")}
        {kind === "cpu" ? (
          <span className="ml-1.5 font-normal text-muted-foreground">{t("usage.coresUnit")}</span>
        ) : null}
      </span>
      <Suspense
        fallback={<Skeleton className="h-44 w-full" aria-label={t("usage.loadingChart")} />}
      >
        <HistoryChart
          times={history.times}
          series={series(kind)}
          gone={gone}
          guides={guides(kind)}
          restarts={restarts}
          format={kind === "cpu" ? f.cpu : f.memory}
          tick={kind === "cpu" ? f.coresExact : f.memory}
          unit={kind === "cpu" ? 1 : 2 ** 20}
          label={t(
            "usage.chartLabel",
            kind === "cpu" ? t("resources.cpu") : t("resources.memory"),
            t(`usage.rangeLong.${range}`),
          )}
          span={span}
        />
      </Suspense>
    </div>
  )
  return (
    <div className="flex flex-col gap-4">
      <div className="grid grid-cols-1 gap-x-8 gap-y-5 xl:grid-cols-2">
        {chart("cpu")}
        {chart("memory")}
      </div>
      <div className="flex flex-wrap items-center justify-between gap-x-6 gap-y-2">
        <Legend series={series("memory")} gone={gone} />
        <ul className="flex flex-wrap items-center gap-x-4 gap-y-1 type-meta text-muted-foreground">
          {history.restarts ? (
            <>
              <li className="flex items-center gap-1.5">
                <span className="size-2 rounded-full bg-fail" aria-hidden="true" />
                {t("usage.markerOom")}
              </li>
              <li className="flex items-center gap-1.5">
                <span className="size-2 rounded-full bg-warn" aria-hidden="true" />
                {t("usage.markerRestart")}
              </li>
            </>
          ) : (
            <li className="flex items-center gap-1.5">
              {t("usage.restartsUnknown")}
              <Hint label={t("usage.restartsUnknown")}>{t("usage.restartsUnknownHint")}</Hint>
            </li>
          )}
        </ul>
      </div>
      <p className="flex flex-wrap items-center gap-x-2 gap-y-1 border-t pt-3 type-meta text-muted-foreground">
        <span className="flex max-w-full min-w-0 items-baseline gap-1">
          {/* The space keeps "From <store>" one phrase in the text, which flex alone would not. */}
          {t("usage.source")}{" "}
          <span className="min-w-0 truncate font-mono text-foreground" translate="no" title={store}>
            {store}
          </span>
        </span>
        <span>{stepText}</span>
        <Hint label={t("usage.tenancy")}>{t("usage.tenancyHint")}</Hint>
      </p>
    </div>
  )
}

function Legend({ series, gone }: { series: ServiceHistory["memory"]; gone: Set<string> }) {
  const t = useT()
  const strokes = colours(series, gone)
  const goneCount = series.filter((s) => gone.has(s.pod)).length
  return (
    <ul className="flex flex-wrap items-center gap-x-4 gap-y-1 type-meta text-muted-foreground">
      {series.map((s, i) =>
        gone.has(s.pod) ? null : (
          <li key={s.pod} className="flex items-center gap-1.5">
            <span
              className={cn(
                "h-0.5 w-3 rounded-full",
                s.pod === "total" ? "bg-progress" : "bg-(--swatch)",
              )}
              style={{ "--swatch": strokes[i] }}
            />
            <span className="font-mono" translate="no">
              {s.pod === "total" ? t("usage.total") : s.pod}
            </span>
          </li>
        ),
      )}
      {goneCount > 0 ? <li>{t("usage.replacedPods", goneCount)}</li> : null}
    </ul>
  )
}
