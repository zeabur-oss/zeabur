import { useQuery } from "@tanstack/react-query"
import { useEffect, useRef, useState, type ReactNode } from "react"
import CaretRightIcon from "~icons/ph/caret-right"

import { toProblem } from "@/api/client"
import { followUrl, logsQuery, type LogOptions } from "@/api/queries"
import { Problem } from "@/components/problem"
import { StatusDot } from "@/components/status"
import { useT } from "@/i18n"
import { cn } from "@/lib/utils"
import { readBuildLog, stepSeconds, type LogLine, type Step } from "@/model/build-steps"
import type { BuildRun } from "@/model/builds"
import type { Health } from "@/model/service"
import { useLogStream } from "@/routes/service/log-stream"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur-oss/design-system/components/collapsible"
import { Skeleton } from "@zeabur-oss/design-system/components/skeleton"

type Phase = BuildRun["phases"][number]

const TAIL = 5000

const options = (container: string): LogOptions => ({
  container,
  tailLines: TAIL,
  previous: false,
  timestamps: false,
})

/** The time now, ticking once a second while `live`. */
function useNow(live: boolean) {
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    if (!live) return
    const timer = window.setInterval(() => setNow(Date.now()), 1000)
    return () => window.clearInterval(timer)
  }, [live])
  return now
}

function Duration({ seconds }: { seconds?: number }) {
  const t = useT()
  if (seconds === undefined) return null
  return (
    <span className="shrink-0 text-xs text-tertiary tabular-nums">
      {t("builds.duration", seconds)}
    </span>
  )
}

const phaseHealth: Record<string, Health> = {
  running: "progress",
  succeeded: "ok",
  failed: "fail",
}

const stepHealth: Record<Step["state"], Health> = {
  running: "progress",
  done: "ok",
  cached: "idle",
  failed: "fail",
}

function phaseSeconds(phase: Phase, now: number) {
  if (!phase.started) return undefined
  const end = phase.finished ? Date.parse(phase.finished) : now
  const ms = end - Date.parse(phase.started)
  return Number.isFinite(ms) && ms >= 0 ? ms / 1000 : undefined
}

const phaseKeys = {
  fetch: "builds.phase.fetch",
  plan: "builds.phase.plan",
  build: "builds.phase.build",
} as const

function PhaseName({ container }: { container: string }) {
  const t = useT()
  const key = phaseKeys[container as keyof typeof phaseKeys]
  return <span className="min-w-0 truncate text-main">{key ? t(key) : container}</span>
}

/** A run's phases, the containers of its pod, each with its log or its steps. */
export function RunSteps({
  project,
  pod,
  phases,
}: {
  project: string
  pod: string
  phases: Phase[]
}) {
  const now = useNow(phases.some((p) => p.state === "running"))
  return (
    <ol className="mt-2 flex min-w-0 flex-col gap-1">
      {phases.map((phase) => (
        <li key={phase.container} className="min-w-0">
          <PhaseRow project={project} pod={pod} phase={phase} now={now} />
        </li>
      ))}
    </ol>
  )
}

function PhaseRow({
  project,
  pod,
  phase,
  now,
}: {
  project: string
  pod: string
  phase: Phase
  now: number
}) {
  const head = (
    <>
      <StatusDot health={phaseHealth[phase.state] ?? "idle"} />
      <PhaseName container={phase.container} />
      <Duration seconds={phaseSeconds(phase, now)} />
    </>
  )
  if (phase.state === "waiting")
    return <div className="flex min-w-0 items-center gap-2 py-1 pl-4.5 text-sm">{head}</div>
  const live = phase.state === "running"
  return (
    <LogSource project={project} pod={pod} container={phase.container} live={live}>
      {(lines) =>
        phase.container === "build" ? (
          <BuildPhase
            head={head}
            lines={lines}
            live={live}
            failed={phase.state === "failed"}
            now={now}
          />
        ) : (
          <Fold head={head} open={phase.state !== "succeeded"}>
            <LogBox lines={lines.map((text) => ({ text, stderr: false }))} live={live} />
          </Fold>
        )
      }
    </LogSource>
  )
}

/** The build container: its steps, or its plain log from a builder that writes no events. */
function BuildPhase({
  head,
  lines,
  live,
  failed,
  now,
}: {
  head: ReactNode
  lines: string[]
  live: boolean
  failed: boolean
  now: number
}) {
  const t = useT()
  const log = readBuildLog(lines)
  const plain = log.other.map((text) => ({ text, stderr: false }))
  if (log.steps.length === 0)
    return (
      <Fold head={head} open={live || failed}>
        <LogBox lines={plain} live={live} />
      </Fold>
    )
  return (
    <div className="flex min-w-0 flex-col">
      <div className="flex min-w-0 items-center gap-2 py-1 pl-4.5 text-sm">{head}</div>
      <ol className="ml-5 flex min-w-0 flex-col border-l border-main pl-3">
        {log.steps.map((step) => (
          <li key={step.key} className="min-w-0" data-testid="build-step" data-step={step.key}>
            <StepRow step={step} live={live} now={now} />
          </li>
        ))}
        {plain.length > 0 ? (
          <li className="min-w-0">
            <Fold
              head={<span className="text-main">{t("builds.otherOutput")}</span>}
              open={failed && !log.failed}
            >
              <LogBox lines={plain} live={false} />
            </Fold>
          </li>
        ) : null}
      </ol>
    </div>
  )
}

function StepRow({ step, live, now }: { step: Step; live: boolean; now: number }) {
  const t = useT()
  const name =
    step.kind === "own" ? (
      <span
        className="min-w-0 truncate font-mono text-xs text-main"
        title={step.name}
        translate="no"
      >
        {step.name}
      </span>
    ) : (
      <span className="min-w-0 truncate text-main">{t(`builds.stepGroup.${step.kind}`)}</span>
    )
  const head = (
    <>
      <StatusDot health={stepHealth[step.state]} />
      {name}
      {step.state === "cached" ? (
        <span className="shrink-0 text-xs text-tertiary">{t("builds.cached")}</span>
      ) : (
        <Duration seconds={stepSeconds(step, now)} />
      )}
    </>
  )
  if (step.lines.length === 0 && !step.error)
    return <div className="flex min-w-0 items-center gap-2 py-1 pl-4.5 text-sm">{head}</div>
  const running = step.state === "running"
  const lines = step.error ? [...step.lines, { text: step.error, stderr: true }] : step.lines
  return (
    <Fold head={head} open={step.state === "failed" || running}>
      <LogBox lines={lines} live={live && running} />
    </Fold>
  )
}

/** A row that folds its log; it opens by itself when `open` turns true, as a step starts running. */
function Fold({ head, open, children }: { head: ReactNode; open: boolean; children: ReactNode }) {
  const [shown, setShown] = useState(open)
  const [was, setWas] = useState(open)
  if (open !== was) {
    setWas(open)
    if (open) setShown(true)
  }
  return (
    <div className="min-w-0 text-sm">
      <Collapsible open={shown} onOpenChange={setShown}>
        <CollapsibleTrigger variant="disclosure" className="my-1 w-full">
          <CaretRightIcon aria-hidden="true" />
          <span className="flex min-w-0 flex-1 items-center gap-2">{head}</span>
        </CollapsibleTrigger>
        <CollapsibleContent>{shown ? children : null}</CollapsibleContent>
      </Collapsible>
    </div>
  )
}

/** The lines, wrapped; while `live` it keeps to the bottom until the reader scrolls up. */
function LogBox({ lines, live }: { lines: LogLine[]; live: boolean }) {
  const t = useT()
  const box = useRef<HTMLPreElement>(null)
  const [stick, setStick] = useState(true)
  useEffect(() => {
    const element = box.current
    if (live && stick && element) element.scrollTop = element.scrollHeight
  }, [lines.length, live, stick])
  const onScroll = () => {
    const element = box.current
    if (!element) return
    const atBottom = element.scrollHeight - element.scrollTop - element.clientHeight < 24
    if (atBottom !== stick) setStick(atBottom)
  }
  return (
    // oxlint-disable-next-line jsx-a11y/no-noninteractive-tabindex -- the log scrolls, so it takes focus
    <pre
      ref={box}
      tabIndex={0}
      onScroll={onScroll}
      className="my-1 ml-4.5 max-h-96 overflow-auto rounded-lg border border-main bg-muted p-3 font-mono text-xs leading-5 break-words whitespace-pre-wrap text-main"
      translate="no"
    >
      {lines.length === 0
        ? t("builds.emptyLog")
        : lines.map((line, i) => (
            // oxlint-disable-next-line react/no-array-index-key -- lines only grow at the end
            <span key={i} className={cn("block", line.title && "font-medium text-secondary")}>
              {line.text}
            </span>
          ))}
    </pre>
  )
}

/** A container's log: followed while it runs, read once after. */
function LogSource(props: {
  project: string
  pod: string
  container: string
  live: boolean
  children: (lines: string[]) => ReactNode
}) {
  return props.live ? <Followed {...props} /> : <Fetched {...props} />
}

function Followed({
  project,
  pod,
  container,
  children,
}: {
  project: string
  pod: string
  container: string
  children: (lines: string[]) => ReactNode
}) {
  const url = followUrl(project, pod, options(container))
  return (
    <Stream key={url} url={url}>
      {children}
    </Stream>
  )
}

function Stream({ url, children }: { url: string; children: (lines: string[]) => ReactNode }) {
  const stream = useLogStream(url)
  return (
    <>
      {children(stream.lines)}
      {stream.problem && stream.problem.status !== 404 ? (
        <Problem problem={stream.problem} />
      ) : null}
    </>
  )
}

function Fetched({
  project,
  pod,
  container,
  children,
}: {
  project: string
  pod: string
  container: string
  children: (lines: string[]) => ReactNode
}) {
  const t = useT()
  const logs = useQuery(logsQuery(project, pod, options(container)))
  if (logs.isPending) return <Skeleton variant="card" className="my-1 h-7 w-full" />
  if (logs.isError)
    return toProblem(logs.error).unavailableLog ? (
      <p className="py-1 text-sm text-secondary">{t("builds.noLogsYet")}</p>
    ) : (
      <Problem error={logs.error} />
    )
  return <>{children(logs.data.lines)}</>
}

/** The step a failed run stopped at, read from its build container's log while its pod is there. */
export function useFailedStep(project: string, run: BuildRun | undefined) {
  const build = run?.phases.find((p) => p.container === "build")
  const enabled = !!run?.pod && build?.state === "failed"
  const logs = useQuery({ ...logsQuery(project, run?.pod ?? "", options("build")), enabled })
  return enabled && logs.data ? readBuildLog(logs.data.lines).failed : undefined
}
