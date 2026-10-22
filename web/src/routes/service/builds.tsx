import { useQuery } from "@tanstack/react-query"
import { lazy, Suspense, useState } from "react"
import ArrowClockwiseIcon from "~icons/ph/arrow-clockwise"
import CubeIcon from "~icons/ph/cube"
import GitBranchIcon from "~icons/ph/git-branch"
import RocketLaunchIcon from "~icons/ph/rocket-launch"

import { toProblem } from "@/api/client"
import { buildsQuery, capabilitiesQuery, logsQuery } from "@/api/queries"
import { Ago, CopyButton, Segmented } from "@/components/bits"
import { LoadingRows } from "@/components/page"
import { usePlanner } from "@/components/plan/planner"
import { Problem } from "@/components/problem"
import { Alert, AlertAction, AlertDescription } from "@zeabur/design-system/components/alert"
import { Button } from "@zeabur/design-system/components/button"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur/design-system/components/collapsible"
import { SectionHeader } from "@zeabur/design-system/components/section-header"
import { Skeleton } from "@zeabur/design-system/components/skeleton"
import {
  SummaryCard,
  SummaryCardActions,
  SummaryCardFact,
  SummaryCardFacts,
  SummaryCardMain,
  SummaryCardSince,
  SummaryCardStatus,
  SummaryCardWord,
} from "@zeabur/design-system/components/summary-card"
import { Switch } from "@zeabur/design-system/components/switch"
import { Timeline, TimelineItem } from "@zeabur/design-system/components/timeline"
import { Tooltip } from "@zeabur/design-system/components/tooltip"
import { useI18n, useT } from "@/i18n"
import { formatAgo } from "@/lib/format"
import {
  builtContainers,
  durationOf,
  KEEL_WEBHOOK,
  keelPolicy,
  runsLatest,
  runsOf,
  runTone,
  shortSha,
  type Build,
  type BuildRun,
  type Builds,
  type Built,
} from "@/model/builds"
import { imageTag, isTask, type Service } from "@/model/service"

const InstallSheet = lazy(() =>
  import("@/routes/cluster/install-sheet").then((m) => ({ default: m.InstallSheet })),
)

/** The service's builds, when a Zeabur Builder `Build` makes one of its images. */
export function useServiceBuilds(project: string, service: Service) {
  const builds = useQuery(buildsQuery(project))
  const built = builds.data ? builtContainers(service, builds.data.builds) : []
  return { builds, built }
}

export function BuildsTab({ project, service }: { project: string; service: Service }) {
  const t = useT()
  const { builds, built } = useServiceBuilds(project, service)
  const planner = usePlanner(project)
  if (builds.isPending) return <LoadingRows label={t("common.loading")} rows={4} />
  if (builds.isError) return <Problem error={builds.error} />
  return (
    <div className="flex flex-col gap-8">
      {built.map((b) => (
        <BuildView
          key={`${b.container}/${b.build.name}`}
          project={project}
          service={service}
          built={b}
          data={builds.data}
          several={built.length > 1}
          plan={planner.plan}
        />
      ))}
      {planner.dialog}
    </div>
  )
}

type Plan = ReturnType<typeof usePlanner>["plan"]

function BuildView({
  project,
  service,
  built,
  data,
  several,
  plan,
}: {
  project: string
  service: Service
  built: Built
  data: Builds
  several: boolean
  plan: Plan
}) {
  const t = useT()
  const { build } = built
  const runs = runsOf(data.runs, build)
  const latest = runs[0]
  const managed = service.provenance.tool ?? undefined
  const deploy = () =>
    build.latestImage &&
    plan({
      title: t("title.deployBuild", service.name),
      changes: [
        {
          op: "image",
          workload: { kind: service.kind, name: service.name },
          container: several ? built.container : undefined,
          image: build.latestImage,
        },
      ],
    })
  const rebuild = () =>
    plan({ title: t("title.rebuild", build.name), changes: [{ op: "rebuild", build: build.name }] })
  return (
    <>
      <Summary
        service={service}
        built={built}
        latest={latest}
        several={several}
        managed={managed}
        onDeploy={deploy}
        onRebuild={rebuild}
      />
      {isTask(service.kind) ? null : (
        <AutoUpdate service={service} build={build} data={data} managed={managed} plan={plan} />
      )}
      <History project={project} runs={runs} />
    </>
  )
}

const words = {
  Pending: "builds.word.pending",
  Planning: "builds.word.planning",
  Building: "builds.word.building",
  Succeeded: "builds.word.succeeded",
  Failed: "builds.word.failed",
  Cancelled: "builds.word.cancelled",
} as const

function phaseWord(t: ReturnType<typeof useT>, run?: BuildRun) {
  const key = run?.phase ? words[run.phase as keyof typeof words] : undefined
  return key ? t(key) : t("builds.word.none")
}

const tones = { ok: "ok", fail: "fail", progress: "progress", neutral: "idle" } as const

function Summary({
  service,
  built,
  latest,
  several,
  managed,
  onDeploy,
  onRebuild,
}: {
  service: Service
  built: Built
  latest?: BuildRun
  several: boolean
  managed?: string
  onDeploy: () => void
  onRebuild: () => void
}) {
  const t = useT()
  const { locale } = useI18n()
  const { build, image } = built
  const current = runsLatest(image, build)
  const since = latest?.started ?? latest?.created
  const deployButton = (
    <Button
      variant={current === false ? "default" : "outline"}
      size="sm"
      disabled={!build.latestImage || current === true || !!managed}
      onClick={onDeploy}
    >
      <RocketLaunchIcon aria-hidden="true" data-icon="inline-start" />
      {t("builds.deploy")}
    </Button>
  )
  return (
    <SummaryCard aria-label={t("builds.status", build.name)}>
      <SummaryCardMain>
        <SummaryCardStatus tone={latest ? tones[runTone(latest)] : "idle"}>
          <SummaryCardWord>{phaseWord(t, latest)}</SummaryCardWord>
        </SummaryCardStatus>
        {since ? (
          <SummaryCardSince>
            {t("builds.since", formatAgo(since, locale), shortSha(latest?.revision) ?? "")}
          </SummaryCardSince>
        ) : null}
        <p className="mt-1 max-w-prose text-sm text-secondary">
          {current === true
            ? t("builds.current", service.name)
            : current === false && build.latestImage
              ? t("builds.behind", service.name, imageTag(build.latestImage))
              : t("builds.noImage")}
        </p>
        {build.ready === "False" && build.message ? (
          <p className="mt-1 max-w-prose text-sm text-fail-strong" lang="en" translate="no">
            {build.message}
          </p>
        ) : latest?.phase === "Failed" && latest.message ? (
          <p
            className="mt-1 max-w-prose text-sm break-words text-fail-strong"
            lang="en"
            translate="no"
          >
            {latest.message}
          </p>
        ) : null}
        {build.suspended ? (
          <p className="mt-1 text-sm text-warn-strong">{t("builds.suspended")}</p>
        ) : null}
        <SummaryCardFacts>
          {build.gitUrl ? (
            <SummaryCardFact label={t("overview.source")}>
              <GitBranchIcon aria-hidden="true" />
              <span className="truncate font-mono" title={build.gitUrl} translate="no">
                {build.gitUrl.replace(/^https?:\/\//, "")}
                {build.gitRef ? `@${build.gitRef}` : ""}
              </span>
            </SummaryCardFact>
          ) : null}
          {build.latestImage ? (
            <SummaryCardFact label={t("builds.image")}>
              <CubeIcon aria-hidden="true" />
              <span className="truncate font-mono" title={build.latestImage} translate="no">
                {imageTag(build.latestImage)}
              </span>
              <CopyButton value={build.latestImage} />
            </SummaryCardFact>
          ) : null}
          {several ? (
            <SummaryCardFact label={t("builds.container")}>
              <span className="truncate font-mono" translate="no">
                {built.container}
              </span>
            </SummaryCardFact>
          ) : null}
          {build.builder ? (
            <SummaryCardFact label={t("builds.builder")}>
              <span className="truncate font-mono" translate="no">
                {build.builder}
              </span>
            </SummaryCardFact>
          ) : null}
        </SummaryCardFacts>
        <SummaryCardActions>
          {managed ? (
            <Tooltip content={t("builds.managedDeploy", managed)}>
              <span>{deployButton}</span>
            </Tooltip>
          ) : (
            deployButton
          )}
          <Button variant="outline" size="sm" onClick={onRebuild}>
            <ArrowClockwiseIcon aria-hidden="true" data-icon="inline-start" />
            {t("builds.rebuild")}
          </Button>
        </SummaryCardActions>
      </SummaryCardMain>
    </SummaryCard>
  )
}

function AutoUpdate({
  service,
  build,
  data,
  managed,
  plan,
}: {
  service: Service
  build: Build
  data: Builds
  managed?: string
  plan: Plan
}) {
  const t = useT()
  const [installing, setInstalling] = useState(false)
  const capabilities = useQuery({ ...capabilitiesQuery, retry: false })
  const keel = capabilities.data?.items.find((c) => c.name === "image-updates")
  const missing = keel !== undefined && !keel.existing
  const policy = keelPolicy(data, service)
  const on = policy !== undefined
  const toggle = (enabled: boolean) =>
    plan({
      title: t(enabled ? "title.autoUpdateOn" : "title.autoUpdateOff", service.name),
      changes: [
        {
          op: "autoUpdate",
          workload: { kind: service.kind, name: service.name },
          enabled,
          build: enabled ? build.name : undefined,
          webhook: enabled ? (build.keelWebhooks[0] ?? KEEL_WEBHOOK) : undefined,
        },
      ],
    })
  const control = (
    <Switch
      checked={on}
      disabled={!!managed || (missing && !on)}
      onCheckedChange={toggle}
      aria-labelledby={`auto-update-${build.name}`}
    />
  )
  return (
    <section className="flex flex-col gap-3">
      <div className="flex items-start gap-3">
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          <h2 id={`auto-update-${build.name}`} className="text-sm font-medium">
            {t("builds.autoUpdate")}
          </h2>
          <p className="text-sm text-secondary">
            {policy && policy !== "force"
              ? t("builds.autoUpdatePolicy", policy)
              : t("builds.autoUpdateHint")}
          </p>
        </div>
        {managed ? (
          <Tooltip content={t("builds.managedDeploy", managed)}>
            <span>{control}</span>
          </Tooltip>
        ) : (
          control
        )}
      </div>
      {missing ? (
        <Alert variant="info">
          <AlertDescription>{t("builds.keelMissing")}</AlertDescription>
          <AlertAction>
            <Button variant="outline" size="xs" onClick={() => setInstalling(true)}>
              {t("builds.installKeel")}
            </Button>
          </AlertAction>
        </Alert>
      ) : null}
      {installing ? (
        <Suspense fallback={null}>
          <InstallSheet
            component="image-updates/keel"
            name="Keel"
            onProfile={() => undefined}
            onClose={() => setInstalling(false)}
          />
        </Suspense>
      ) : null}
    </section>
  )
}

const triggers = {
  webhook: "builds.trigger.webhook",
  poll: "builds.trigger.poll",
  manual: "builds.trigger.manual",
  baseImage: "builds.trigger.baseImage",
  replan: "builds.trigger.replan",
} as const

function History({ project, runs }: { project: string; runs: BuildRun[] }) {
  const t = useT()
  const [now] = useState(() => Date.now())
  if (runs.length === 0) return null
  return (
    <section aria-labelledby="builds-history" className="flex flex-col gap-4">
      <SectionHeader id="builds-history" title={t("builds.history")} />
      <Timeline aria-labelledby="builds-history">
        {runs.map((run) => {
          const ms = durationOf(run, now)
          const trigger = run.trigger ? triggers[run.trigger as keyof typeof triggers] : undefined
          return (
            <TimelineItem
              key={run.name}
              tone={runTone(run)}
              time={<Ago iso={run.started ?? run.created} />}
              title={
                <span className="flex flex-wrap items-baseline gap-x-2">
                  {phaseWord(t, run)}
                  {run.revision ? (
                    <span className="font-mono text-xs font-normal text-secondary" translate="no">
                      {shortSha(run.revision)}
                    </span>
                  ) : null}
                </span>
              }
              meta={
                [
                  trigger ? t(trigger) : undefined,
                  ms === undefined ? undefined : t("builds.took", Math.round(ms / 1000)),
                  run.reason && run.phase === "Failed" ? run.reason : undefined,
                ]
                  .filter(Boolean)
                  .join(t("common.sep")) || undefined
              }
            >
              <RunLogs project={project} run={run} />
            </TimelineItem>
          )
        })}
      </Timeline>
    </section>
  )
}

function RunLogs({ project, run }: { project: string; run: BuildRun }) {
  const t = useT()
  const [open, setOpen] = useState(false)
  return (
    <Collapsible open={open} onOpenChange={setOpen}>
      <CollapsibleTrigger render={<Button variant="ghost" size="xs" className="-ml-2" />}>
        {t(open ? "builds.hideLogs" : "builds.logs")}
      </CollapsibleTrigger>
      <CollapsibleContent>
        {open ? (
          run.pod ? (
            <PodLogs project={project} pod={run.pod} containers={run.containers} />
          ) : (
            <p className="mt-2 text-sm text-secondary">
              {run.message ? (
                <span className="mb-2 block font-mono text-xs break-words" lang="en" translate="no">
                  {run.message}
                </span>
              ) : null}
              {t("builds.logsGone")}
            </p>
          )
        ) : null}
      </CollapsibleContent>
    </Collapsible>
  )
}

function PodLogs({
  project,
  pod,
  containers,
}: {
  project: string
  pod: string
  containers: string[]
}) {
  const t = useT()
  const [container, setContainer] = useState(
    containers.includes("build") ? "build" : (containers.at(-1) ?? ""),
  )
  const logs = useQuery(
    logsQuery(project, pod, { container, tailLines: 2000, previous: false, timestamps: false }),
  )
  return (
    <div className="mt-2 flex min-w-0 flex-col gap-2">
      {containers.length > 1 ? (
        <Segmented
          label={t("builds.step")}
          value={container}
          options={containers.map((c) => ({ value: c, label: c }))}
          onChange={setContainer}
        />
      ) : null}
      {logs.isPending ? (
        <Skeleton variant="card" className="h-40 w-full" />
      ) : logs.isError ? (
        toProblem(logs.error).unavailableLog ? (
          <p className="text-sm text-secondary">{t("builds.noLogsYet")}</p>
        ) : (
          <Problem error={logs.error} />
        )
      ) : (
        <pre
          className="max-h-96 overflow-auto rounded-lg border border-main bg-muted p-3 font-mono text-xs leading-5 text-main"
          translate="no"
        >
          {logs.data.lines.join("\n") || t("builds.emptyLog")}
        </pre>
      )}
    </div>
  )
}
