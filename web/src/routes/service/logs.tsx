// A pod's log, filling the remaining height: pod picker (ready, reason, restarts; the pod in the
// URL, else a failing one), container, previous instance, a filter box with counted severity
// chips, Follow, wrap, timestamps, download, and a small severity histogram. Severity is a thin
// left bar, never the whole row. Lines are fetched with timestamps for the histogram; they are
// shown only when asked. Loaded on first use (the virtual list is not in the first page load).
import { useQuery } from "@tanstack/react-query"
import { useVirtualizer } from "@tanstack/react-virtual"
import { useEffect, useRef, useState, type ReactNode } from "react"
import ArrowClockwiseIcon from "~icons/ph/arrow-clockwise"
import ArrowDownIcon from "~icons/ph/arrow-down"
import BroadcastIcon from "~icons/ph/broadcast"
import BroadcastFillIcon from "~icons/ph/broadcast-fill"
import CaretDownIcon from "~icons/ph/caret-down"
import CheckIcon from "~icons/ph/check"
import ClockIcon from "~icons/ph/clock"
import ClockCounterClockwiseIcon from "~icons/ph/clock-counter-clockwise"
import DownloadSimpleIcon from "~icons/ph/download-simple"
import FunnelIcon from "~icons/ph/funnel"
import TextAlignLeftIcon from "~icons/ph/text-align-left"

import { toProblem } from "@/api/client"
import { followUrl, logsQuery, type LogOptions } from "@/api/queries"
import { Histogram } from "@/components/bits"
import { Problem } from "@/components/problem"
import { StatusDot } from "@/components/status"
import { Button } from "@engawahq/design-system/components/button"
import { Popover, PopoverContent, PopoverTrigger } from "@engawahq/design-system/components/popover"
import { Skeleton } from "@engawahq/design-system/components/skeleton"
import { Toggle } from "@engawahq/design-system/components/toggle"
import { useI18n, useT } from "@/i18n"
import { parseAnsi } from "@/lib/ansi"
import { cn } from "@/lib/utils"
import { podHealth, podReason, worstPod, type Service } from "@/model/service"
import { useLogStream } from "@/routes/service/log-stream"
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
} from "@engawahq/design-system/components/empty"
import { Illustration } from "@engawahq/design-system/components/illustration"
import { Select } from "@engawahq/design-system/components/select"

type Severity = "error" | "warn" | "info" | "debug"

interface Line {
  raw: string
  text: string
  time?: number
  stamp?: string
  severity: Severity
}

// oxlint-disable-next-line no-control-regex -- stripping escape sequences for matching
const ansi = /\u001b\[[0-?]*[ -/]*[@-~]/g
const stampRe = /^(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?Z)\s/

function parseLine(raw: string): Line {
  const match = raw.match(stampRe)
  const body = match ? raw.slice(match[0].length) : raw
  const plain = body.replace(ansi, "")
  const severity: Severity = /\b(error|err|fatal|panic|exception)\b/i.test(plain)
    ? "error"
    : /\b(warn|warning)\b/i.test(plain)
      ? "warn"
      : /\b(debug|trace)\b/i.test(plain)
        ? "debug"
        : "info"
  return {
    raw: body,
    text: plain,
    time: match?.[1] ? Date.parse(match[1]) : undefined,
    stamp: match?.[1],
    severity,
  }
}

const severityBar: Record<Severity, string> = {
  error: "bg-fail",
  warn: "bg-warn",
  info: "bg-transparent",
  debug: "bg-transparent",
}

export function LogsTab({
  project,
  service,
  pod: selectedPod,
  onPodChange,
}: {
  project: string
  service: Service
  pod?: string
  onPodChange: (pod: string) => void
}) {
  const t = useT()
  const pods = service.pods
  // The failing pod first: that is usually why someone opens the logs.
  const pod = pods.find((p) => p.name === selectedPod) ?? worstPod(service)
  const containerNames =
    pod && pod.containers.length > 0 ? pod.containers : service.containers.map((c) => c.name)
  const [container, setContainer] = useState<string>()
  const chosen = container && containerNames.includes(container) ? container : containerNames[0]
  const [previous, setPrevious] = useState(false)
  const [timestamps, setTimestamps] = useState(false)
  const [follow, setFollow] = useState(false)
  const [wrap, setWrap] = useState(false)
  const [filter, setFilter] = useState("")
  const [severity, setSeverity] = useState<Severity | "all">("all")
  const [podOpen, setPodOpen] = useState(false)

  if (!pod) {
    return (
      <div className="flex flex-1 items-center justify-center rounded-xl border border-dashed p-8 text-sm text-muted-foreground">
        {t("logs.noPods")}
      </div>
    )
  }

  const options: LogOptions = {
    container: containerNames.length > 1 ? chosen : undefined,
    tailLines: 1000,
    previous,
    timestamps: true,
  }

  const toolbar = (
    <div className="flex flex-wrap items-center gap-2">
      <Popover open={podOpen} onOpenChange={setPodOpen}>
        <PopoverTrigger
          render={<Button variant="outline" size="sm" className="max-w-full justify-start" />}
          aria-label={t("logs.podOf", pod.name)}
        >
          <StatusDot health={podHealth(pod)} />
          <span className="truncate font-mono text-xs">{pod.name}</span>
          <span className="text-muted-foreground tabular-nums">↻{pod.restarts}</span>
          <CaretDownIcon aria-hidden="true" data-icon="inline-end" />
        </PopoverTrigger>
        <PopoverContent className="w-80">
          <ul className="flex flex-col">
            {pods.map((p) => (
              <li key={p.name}>
                <button
                  type="button"
                  onClick={() => {
                    onPodChange(p.name)
                    setPodOpen(false)
                  }}
                  className="flex h-9 w-full items-center gap-2 rounded-lg px-2 text-left text-xs hover:bg-muted"
                >
                  <StatusDot health={podHealth(p)} />
                  <span className="truncate font-mono">{p.name}</span>
                  <span
                    className={cn(
                      "rounded px-1 type-nano font-medium",
                      p.ready ? "bg-ok/12 text-ok" : "bg-fail/10 text-fail",
                    )}
                  >
                    {p.ready ? t("pod.ready") : (podReason(p) ?? t("pod.notReady"))}
                  </span>
                  <span className="ml-auto text-muted-foreground tabular-nums">
                    {t("pod.restarts", p.restarts)}
                  </span>
                  {p.name === pod.name ? (
                    <CheckIcon className="size-3.5" aria-hidden="true" />
                  ) : null}
                </button>
              </li>
            ))}
          </ul>
        </PopoverContent>
      </Popover>
      {containerNames.length > 1 ? (
        <Select
          size="sm"
          value={chosen}
          onValueChange={setContainer}
          aria-label={t("logs.container")}
          translate="no"
          items={containerNames.map((name) => ({ value: name, label: name }))}
        />
      ) : null}
      <Toggle variant="outline" size="sm" pressed={previous} onPressedChange={setPrevious}>
        <ClockCounterClockwiseIcon aria-hidden="true" />
        {t("logs.previous")}
      </Toggle>
      <label className="flex h-7 min-w-40 flex-1 items-center gap-1.5 rounded-lg border bg-background px-2 focus-within:border-ring">
        <FunnelIcon className="size-3.5 text-muted-foreground" aria-hidden="true" />
        <input
          value={filter}
          onChange={(event) => setFilter(event.target.value)}
          aria-label={t("logs.filter")}
          title={t("logs.filterHint")}
          className="w-full bg-transparent text-xs outline-none placeholder:text-muted-foreground"
        />
      </label>
      <div className="flex items-center gap-1">
        <Toggle
          variant="outline"
          size="sm"
          pressed={wrap}
          onPressedChange={setWrap}
          aria-label={t("logs.wrap")}
          title={t("logs.wrap")}
        >
          <TextAlignLeftIcon aria-hidden="true" />
        </Toggle>
        <Toggle
          variant="outline"
          size="sm"
          pressed={timestamps}
          onPressedChange={setTimestamps}
          aria-label={t("logs.timestamps")}
          title={t("logs.timestamps")}
        >
          <ClockIcon aria-hidden="true" />
        </Toggle>
        <Toggle variant="outline" size="sm" pressed={follow} onPressedChange={setFollow}>
          {follow ? <BroadcastFillIcon aria-hidden="true" /> : <BroadcastIcon aria-hidden="true" />}
          {t("logs.follow")}
        </Toggle>
      </div>
    </div>
  )

  const viewProps = {
    filter,
    severity,
    onSeverity: setSeverity,
    wrap,
    timestamps,
    fileName: `${pod.name}${chosen ? `-${chosen}` : ""}.log`,
  }

  return (
    <div className="flex min-h-105 flex-1 flex-col gap-3">
      {toolbar}
      {follow ? (
        <Followed
          key={followUrl(project, pod.name, options)}
          url={followUrl(project, pod.name, options)}
          {...viewProps}
        />
      ) : (
        <Snapshot
          project={project}
          pod={pod.name}
          options={options}
          onCurrentRun={() => setPrevious(false)}
          {...viewProps}
        />
      )}
    </div>
  )
}

interface ViewProps {
  filter: string
  severity: Severity | "all"
  onSeverity: (s: Severity | "all") => void
  wrap: boolean
  timestamps: boolean
  fileName: string
}

function Snapshot({
  project,
  pod,
  options,
  onCurrentRun,
  ...props
}: { project: string; pod: string; options: LogOptions; onCurrentRun: () => void } & ViewProps) {
  const t = useT()
  const logs = useQuery(logsQuery(project, pod, options))
  if (logs.isPending) return <Skeleton variant="card" className="min-h-80 w-full flex-1" />
  if (logs.isError) {
    const problem = toProblem(logs.error)
    if (!problem.unavailableLog) return <Problem problem={problem} />
    // The kubelet had no log to give: a state of the container, not an error, and no line.
    const reason = problem.unavailableLog
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-3 rounded-xl border border-dashed p-8 text-center text-sm">
        <p className="font-medium">{t(`logs.unavailable.${reason}`)}</p>
        <p className="text-muted-foreground">{t(`logs.unavailableFix.${reason}`)}</p>
        <div className="flex gap-2">
          {options.previous ? (
            <Button variant="outline" size="sm" onClick={onCurrentRun}>
              {t("logs.currentRun")}
            </Button>
          ) : null}
          <Button
            variant="ghost"
            size="sm"
            onClick={() => void logs.refetch()}
            disabled={logs.isFetching}
          >
            <ArrowClockwiseIcon aria-hidden="true" data-icon="inline-start" />
            {t("logs.refresh")}
          </Button>
        </div>
      </div>
    )
  }
  return (
    <LogView
      lines={logs.data.lines}
      {...props}
      status={
        <Button
          variant="ghost"
          size="xs"
          onClick={() => void logs.refetch()}
          disabled={logs.isFetching}
        >
          <ArrowClockwiseIcon aria-hidden="true" data-icon="inline-start" />
          {t("logs.refresh")}
        </Button>
      }
    />
  )
}

function Followed({ url, ...props }: { url: string } & ViewProps) {
  const t = useT()
  const stream = useLogStream(url)
  return (
    <>
      {stream.problem ? <Problem problem={stream.problem} /> : null}
      <LogView
        lines={stream.lines}
        {...props}
        status={
          <output className="inline-flex items-center gap-1.5 text-xs text-muted-foreground">
            {stream.ended ? (
              t("logs.ended")
            ) : (
              <>
                <span
                  className="size-2 rounded-full bg-ok motion-safe:animate-pulse"
                  aria-hidden="true"
                />
                {t("logs.following")}
              </>
            )}
          </output>
        }
      />
    </>
  )
}

function LogView({
  lines: rawLines,
  status,
  filter,
  severity,
  onSeverity,
  wrap,
  timestamps,
  fileName,
}: { lines: string[]; status: ReactNode } & ViewProps) {
  "use no memo" // TanStack Virtual's instance is mutable; the React Compiler must not memoise it.
  const t = useT()
  const { locale } = useI18n()
  const scroller = useRef<HTMLDivElement>(null)
  const [stick, setStick] = useState(true)
  const lines = rawLines.map(parseLine)
  const needle = filter.trim().toLowerCase()
  const exclude = needle.startsWith("-") ? needle.slice(1) : ""
  const matching = lines.filter((line) =>
    !needle
      ? true
      : exclude
        ? !line.text.toLowerCase().includes(exclude)
        : line.text.toLowerCase().includes(needle),
  )
  const counts: Record<Severity, number> = { error: 0, warn: 0, info: 0, debug: 0 }
  for (const line of matching) counts[line.severity]++
  const shown = severity === "all" ? matching : matching.filter((l) => l.severity === severity)

  // Histogram: by time when lines carry timestamps, else by position.
  const n = 40
  const times = lines.map((l) => l.time).filter((x): x is number => x !== undefined)
  const first = times.length ? Math.min(...times) : 0
  const last = times.length ? Math.max(...times) : 0
  const buckets = Array.from({ length: n }, () => ({ warning: 0, normal: 0 }))
  matching.forEach((line, i) => {
    const pos =
      line.time !== undefined && last > first
        ? (line.time - first) / (last - first)
        : i / Math.max(matching.length - 1, 1)
    const bucket = buckets[Math.min(n - 1, Math.floor(pos * n))]
    if (!bucket) return
    if (line.severity === "error" || line.severity === "warn") bucket.warning++
    else bucket.normal++
  })
  const fmt = (ms: number) =>
    new Intl.DateTimeFormat(locale, { hour: "2-digit", minute: "2-digit" }).format(ms)

  // oxlint-disable-next-line react/incompatible-library -- this component opts out of the compiler
  const virtualizer = useVirtualizer({
    count: shown.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => 20,
    overscan: 20,
  })

  useEffect(() => {
    const element = scroller.current
    if (stick && element) element.scrollTop = element.scrollHeight
  }, [shown.length, stick])

  const onScroll = () => {
    const element = scroller.current
    if (!element) return
    const atBottom = element.scrollHeight - element.scrollTop - element.clientHeight < 24
    if (atBottom !== stick) setStick(atBottom)
  }

  const download = () => {
    const blob = new Blob([rawLines.join("\n")], { type: "text/plain" })
    const href = URL.createObjectURL(blob)
    const a = document.createElement("a")
    a.href = href
    a.download = fileName
    a.click()
    window.setTimeout(() => URL.revokeObjectURL(href), 1000)
  }

  const chip = (value: Severity | "all", label: string, count: number, dot?: string) => (
    <button
      key={value}
      type="button"
      aria-pressed={severity === value}
      onClick={() => onSeverity(value)}
      className={cn(
        "inline-flex h-6 items-center gap-1.5 rounded-md border px-2 text-xs outline-none",
        severity === value
          ? "border-foreground/30 bg-muted font-medium"
          : "text-muted-foreground hover:text-foreground",
      )}
    >
      {dot ? <span className={cn("size-1.5", dot)} aria-hidden="true" /> : null}
      {label}
      <span className="tabular-nums opacity-70">{count}</span>
    </button>
  )

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border bg-card shadow-xs">
      <div className="flex flex-col gap-2 border-b px-3 pt-2.5 pb-2">
        <div className="flex flex-wrap items-center gap-1.5">
          {chip("all", t("common.all"), matching.length)}
          {chip("error", t("logs.error"), counts.error, "bg-fail")}
          {chip("warn", t("logs.warn"), counts.warn, "bg-warn")}
          {chip("info", t("logs.info"), counts.info + counts.debug, "bg-foreground/30")}
          <span className="ml-auto flex items-center gap-1">
            {!stick ? (
              <Button variant="outline" size="xs" onClick={() => setStick(true)}>
                <ArrowDownIcon aria-hidden="true" data-icon="inline-start" />
                {t("logs.jump")}
              </Button>
            ) : null}
            {status}
            <Button
              variant="ghost"
              size="icon-xs"
              onClick={download}
              aria-label={t("logs.download")}
              title={t("logs.download")}
            >
              <DownloadSimpleIcon aria-hidden="true" />
            </Button>
          </span>
        </div>
        <Histogram
          buckets={buckets}
          height="h-8"
          labels={times.length ? [fmt(first), fmt(last)] : undefined}
        />
      </div>
      <div
        ref={scroller}
        onScroll={onScroll}
        role="log"
        aria-label={t("logs.lines")}
        aria-live="off"
        // A scrollable region must take focus so it can be scrolled with the keyboard (WCAG 2.1.1).
        // oxlint-disable-next-line jsx-a11y/no-noninteractive-tabindex
        tabIndex={0}
        className="relative min-h-0 flex-1 overflow-auto bg-muted/30 py-1 font-mono text-xs leading-5 outline-none"
      >
        {shown.length === 0 ? (
          <Empty size="sm">
            <EmptyHeader>
              <EmptyMedia>
                <Illustration
                  kind={rawLines.length === 0 ? "empty" : "no-results"}
                  seed="logs"
                  className="w-24"
                />
              </EmptyMedia>
              <EmptyDescription>
                {rawLines.length === 0 ? t("logs.empty") : t("logs.noMatch")}
              </EmptyDescription>
            </EmptyHeader>
          </Empty>
        ) : (
          <div
            className={cn("relative h-(--total)", wrap ? "w-full" : "w-max min-w-full")}
            style={{ "--total": `${virtualizer.getTotalSize()}px` }}
          >
            {virtualizer.getVirtualItems().map((item) => {
              const line = shown[item.index]
              if (!line) return null
              return (
                <div
                  key={item.key}
                  data-index={item.index}
                  ref={virtualizer.measureElement}
                  className={cn(
                    "absolute top-0 left-0 flex min-w-full translate-y-(--start) gap-3 pr-4 hover:bg-hover",
                    wrap ? "w-full whitespace-pre-wrap break-all" : "w-max whitespace-pre",
                    line.severity === "error" && "bg-fail/[0.04]",
                  )}
                  style={{ "--start": `${item.start}px` }}
                >
                  <span
                    className={cn("w-0.75 shrink-0 self-stretch", severityBar[line.severity])}
                    aria-hidden="true"
                  />
                  {timestamps && line.stamp ? (
                    <span className="shrink-0 text-muted-foreground select-none">
                      {line.stamp.slice(11, 19)}
                    </span>
                  ) : null}
                  <span className="min-w-0">
                    {parseAnsi(line.raw).map((segment) =>
                      segment.className ? (
                        <span key={segment.start} className={segment.className}>
                          {segment.text}
                        </span>
                      ) : (
                        segment.text
                      ),
                    )}
                  </span>
                </div>
              )
            })}
          </div>
        )}
      </div>
      <div className="flex items-center gap-3 border-t px-3 py-1.5 type-micro leading-4 text-muted-foreground tabular-nums">
        <span>{t("logs.count", shown.length)}</span>
        {needle ? <span>{t("logs.of", lines.length)}</span> : null}
      </div>
    </div>
  )
}
