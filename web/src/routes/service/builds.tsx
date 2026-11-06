import { useMutation, useQuery } from "@tanstack/react-query"
import { Link, useParams } from "@tanstack/react-router"
import { useState } from "react"
import ArrowClockwiseIcon from "~icons/ph/arrow-clockwise"
import ArrowSquareOutIcon from "~icons/ph/arrow-square-out"
import CaretRightIcon from "~icons/ph/caret-right"
import CubeIcon from "~icons/ph/cube"
import FolderSimpleIcon from "~icons/ph/folder-simple"
import GitBranchIcon from "~icons/ph/git-branch"
import RocketLaunchIcon from "~icons/ph/rocket-launch"
import UploadSimpleIcon from "~icons/ph/upload-simple"

import { toProblem } from "@/api/client"
import { cn } from "@/lib/utils"
import * as zeabur from "@/api/zeabur"
import { buildsQuery, capabilitiesQuery } from "@/api/queries"
import { Ago, CopyButton } from "@/components/bits"
import { InstallAlert } from "@/components/lazy-installer-sheet"
import { LoadingRows } from "@/components/page"
import { GateTip, useHereGate } from "@/components/gate"
import { usePlanner } from "@/components/plan/planner"
import { FormField } from "@/components/form-field"
import { Problem } from "@/components/problem"
import { UploadFolderDrawer, UploadProgress } from "@/components/upload-folder"
import { Alert, AlertDescription } from "@zeabur-oss/design-system/components/alert"
import { Button } from "@zeabur-oss/design-system/components/button"
import { CopyInput } from "@zeabur-oss/design-system/components/copy-input"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur-oss/design-system/components/collapsible"
import { Empty, EmptyDescription, EmptyHeader } from "@zeabur-oss/design-system/components/empty"
import { SectionHeader } from "@zeabur-oss/design-system/components/section-header"
import {
  SummaryCard,
  SummaryCardActions,
  SummaryCardFact,
  SummaryCardFacts,
  SummaryCardMain,
  SummaryCardSince,
  SummaryCardStatus,
  SummaryCardWord,
} from "@zeabur-oss/design-system/components/summary-card"
import { Switch } from "@zeabur-oss/design-system/components/switch"
import { Timeline, TimelineItem } from "@zeabur-oss/design-system/components/timeline"
import { Tooltip } from "@zeabur-oss/design-system/components/tooltip"
import { useI18n, useT } from "@/i18n"
import { formatAgo } from "@/lib/format"
import {
  branchOf,
  buildProblem,
  building,
  builtContainers,
  durationOf,
  failureOf,
  KEEL_WEBHOOK,
  keelPolicy,
  reasonKey,
  refusedCredentials,
  runsLatest,
  runsOf,
  runTone,
  shortSha,
  type Build,
  type BuildRun,
  type Builds,
  type Built,
} from "@/model/builds"
import { deployKeyPage, parseRepository } from "@/model/git-service"
import { needed } from "@/model/installer"
import { imageTag, isTask, type Service } from "@/model/service"
import { RunSteps, useFailedStep } from "@/routes/service/build-steps"

/** The service's builds, when a Zeabur Builder `Build` makes one of its images. */
export function useServiceBuilds(project: string, service: Service) {
  const builds = useQuery({
    ...buildsQuery(project),
    refetchInterval: (query) =>
      query.state.data && building(query.state.data, Date.now()) ? 5_000 : 30_000,
  })
  const built = builds.data ? builtContainers(service, builds.data.builds) : []
  return { builds, built }
}

export function BuildsTab({ project, service }: { project: string; service: Service }) {
  const t = useT()
  const { builds, built } = useServiceBuilds(project, service)
  const planner = usePlanner(project)
  if (builds.isPending) return <LoadingRows label={t("common.loading")} rows={4} />
  if (builds.isError) return <Problem error={builds.error} />
  if (built.length === 0)
    return (
      <Empty size="sm">
        <EmptyHeader>
          <EmptyDescription>{t("builds.notBuilt")}</EmptyDescription>
        </EmptyHeader>
      </Empty>
    )
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
        project={project}
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

/** Whether the service runs the newest image, in a sentence. */
function currentWords(
  t: ReturnType<typeof useT>,
  service: Service,
  build: Build,
  current: boolean | undefined,
) {
  if (current === true) return t("builds.current", service.name)
  if (current === false && build.latestImage)
    return t("builds.behind", service.name, imageTag(build.latestImage))
  return t("builds.noImage")
}

/** The word for where the build is: waiting for the folder, why it cannot build, or its run. */
function statusWord(
  t: ReturnType<typeof useT>,
  { waiting, problem, latest }: { waiting: boolean; problem: Build["problem"]; latest?: BuildRun },
) {
  if (waiting) return t("builds.word.waitingUpload")
  if (!problem) return phaseWord(t, latest)
  if (!problem.lasting) return t("builds.word.retrying")
  return t(
    problem.condition === "BuilderReady"
      ? "builds.word.builderMissing"
      : "builds.word.sourceFailed",
  )
}

function phaseWord(t: ReturnType<typeof useT>, run?: BuildRun) {
  const key = run?.phase ? words[run.phase as keyof typeof words] : undefined
  return key ? t(key) : t("builds.word.none")
}

const tones = { ok: "ok", fail: "fail", progress: "progress", neutral: "idle" } as const

/** What to do when the Git host refuses the Build's credentials; for a deploy key, the key to add
 * again. */
function Refused({
  project,
  build,
  kind,
}: {
  project: string
  build: Build
  kind: "deployKey" | "token"
}) {
  const t = useT()
  return (
    <div className="mt-1 flex max-w-prose flex-col gap-2">
      <p className="text-sm text-fail-strong">{t(`builds.refused.${kind}`)}</p>
      {kind === "deployKey" ? <DeployKey project={project} build={build} /> : null}
    </div>
  )
}

/** The deploy key to add again, read from its Secret only when asked, and the page to add it on. */
function DeployKey({ project, build }: { project: string; build: Build }) {
  const t = useT()
  const show = useMutation({
    mutationFn: async () => await zeabur.revealDeployKey(project, build.credentials ?? ""),
  })
  const repo = parseRepository(build.gitUrl ?? "")
  const keyPage = repo ? deployKeyPage(repo) : undefined
  return (
    <>
      {show.data ? (
        <FormField label={t("add.deployKeyPublic")}>
          <CopyInput value={show.data.publicKey} copyLabel={t("common.copy")} />
        </FormField>
      ) : (
        <Button
          variant="outline"
          size="sm"
          className="self-start"
          disabled={show.isPending}
          onClick={() => show.mutate()}
        >
          {t("builds.showKey")}
        </Button>
      )}
      {keyPage && repo ? (
        <a
          href={keyPage}
          target="_blank"
          rel="noreferrer"
          className="inline-flex items-center gap-1 self-start text-sm text-foreground hover:underline"
        >
          {t("add.deployKeyOpen", repo.host === "github.com" ? "GitHub" : "GitLab")}
          <ArrowSquareOutIcon aria-hidden="true" className="size-3.5" />
        </a>
      ) : null}
      {show.isError ? (
        <Alert variant="destructive">
          <AlertDescription>{toProblem(show.error).detail}</AlertDescription>
        </Alert>
      ) : null}
    </>
  )
}

function Summary({
  project,
  service,
  built,
  latest,
  several,
  managed,
  onDeploy,
  onRebuild,
}: {
  project: string
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
  const problem = buildProblem(build, latest)
  const branch = branchOf(build)
  const refused = refusedCredentials(build, latest)
  const waiting = build.awaitingUpload && !build.latestImage
  return (
    <SummaryCard aria-label={t("builds.status", build.name)}>
      <SummaryCardMain>
        <SummaryCardStatus tone={summaryTone(problem, latest)}>
          <SummaryCardWord>{statusWord(t, { waiting, problem, latest })}</SummaryCardWord>
        </SummaryCardStatus>
        {since ? (
          <SummaryCardSince>
            {t("builds.since", formatAgo(since, locale), shortSha(latest?.revision) ?? "")}
          </SummaryCardSince>
        ) : null}
        <p className="mt-1 max-w-prose text-sm text-secondary">
          {waiting
            ? t("builds.waitingUpload", service.name)
            : currentWords(t, service, build, current)}
        </p>
        <Failure
          project={project}
          build={build}
          latest={latest}
          refused={!!refused}
          kept={build.latestImage ? service.name : undefined}
        />
        {refused ? <Refused project={project} build={build} kind={refused} /> : null}
        <UploadProgress project={project} build={build.name} />
        {build.suspended ? (
          <p className="mt-1 text-sm text-warn-strong">{t("builds.suspended")}</p>
        ) : null}
        <SummaryFacts built={built} branch={branch} several={several} />
        <SummaryActions
          project={project}
          build={build}
          current={current}
          managed={managed}
          waiting={!!waiting}
          onDeploy={onDeploy}
          onRebuild={onRebuild}
        />
      </SummaryCardMain>
    </SummaryCard>
  )
}

function SummaryActions({
  project,
  build,
  current,
  managed,
  waiting,
  onDeploy,
  onRebuild,
}: {
  project: string
  build: Build
  current: boolean | undefined
  managed?: string
  waiting: boolean
  onDeploy: () => void
  onRebuild: () => void
}) {
  const t = useT()
  const deployGate = useHereGate("changeServices")
  const rebuildGate = useHereGate("build")
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
    <SummaryCardActions>
      {managed ? (
        <Tooltip content={t("builds.managedDeploy", managed)}>
          <span>{deployButton}</span>
        </Tooltip>
      ) : (
        <GateTip gate={deployGate}>{deployButton}</GateTip>
      )}
      {build.archive ? (
        <UploadAction project={project} build={build.name} primary={waiting} />
      ) : null}
      {waiting ? null : (
        <GateTip gate={rebuildGate}>
          <Button variant="outline" size="sm" onClick={onRebuild}>
            <ArrowClockwiseIcon aria-hidden="true" data-icon="inline-start" />
            {t("builds.rebuild")}
          </Button>
        </GateTip>
      )}
    </SummaryCardActions>
  )
}

function summaryTone(problem: ReturnType<typeof buildProblem>, latest?: BuildRun) {
  if (problem) return problem.lasting ? "fail" : "progress"
  return latest ? tones[runTone(latest)] : "idle"
}

function Failure({
  project,
  build,
  latest,
  refused,
  kept,
}: {
  project: string
  build: Build
  latest?: BuildRun
  refused: boolean
  /** The service that keeps running its image. */
  kept?: string
}) {
  const { message, passing, failedRun } = failureOf(build, latest)
  if (message)
    return (
      <p
        className={cn(
          "mt-1 max-w-prose text-sm break-words",
          passing || refused ? "text-secondary" : "text-fail-strong",
        )}
        lang="en"
        translate="no"
      >
        {message}
      </p>
    )
  return failedRun ? <FailedRun project={project} run={failedRun} kept={kept} /> : null
}

function SummaryFacts({
  built,
  branch,
  several,
}: {
  built: Built
  branch: string | undefined
  several: boolean
}) {
  const t = useT()
  const { build } = built
  return (
    <SummaryCardFacts>
      <SourceFact build={build} branch={branch} />
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
  )
}

/** The build ran out of memory: raise the project's build memory, in its settings. */
function OutOfMemory() {
  const t = useT()
  const { project } = useParams({ strict: false }) as { project: string }
  return (
    <p className="text-sm text-fail-strong">
      {t("builds.reason.OutOfMemory")}{" "}
      <Link
        to="/projects/$project/settings"
        params={{ project }}
        hash="builds"
        className="font-medium underline underline-offset-2"
      >
        {t("builds.oom.open")}
      </Link>
    </p>
  )
}

/** Why a run failed: the step it stopped at with its error, else a sentence with the builder's own
 * message behind Details; and that the image from before keeps running. */
function FailedRun({ project, run, kept }: { project: string; run: BuildRun; kept?: string }) {
  const t = useT()
  const step = useFailedStep(project, run)
  return (
    <div className="mt-1 flex max-w-prose flex-col items-start gap-1">
      {step ? (
        <FailedStep step={step} />
      ) : run.reason === "OutOfMemory" ? (
        <OutOfMemory />
      ) : (
        <p className="text-sm text-fail-strong">{t(reasonKey(run.reason))}</p>
      )}
      {kept ? <p className="text-sm text-secondary">{t("builds.keepsRunning", kept)}</p> : null}
      {!step && run.message ? <RunDetails message={run.message} /> : null}
    </div>
  )
}

function FailedStep({ step }: { step: { name: string; seconds?: number; message: string } }) {
  const t = useT()
  return (
    <>
      <p className="text-sm font-medium text-fail-strong">
        {step.seconds === undefined
          ? t("builds.stepFailed", step.name)
          : t("builds.stepFailedAfter", step.name, step.seconds)}
      </p>
      {step.message ? <RunMessage message={step.message} /> : null}
    </>
  )
}

function RunMessage({ message }: { message: string }) {
  return (
    <pre
      className="mt-1 max-h-96 w-full overflow-auto rounded-lg border border-main bg-muted p-3 font-mono text-xs leading-5 break-words whitespace-pre-wrap text-main"
      lang="en"
      translate="no"
    >
      {message}
    </pre>
  )
}

function RunDetails({ message }: { message: string }) {
  const t = useT()
  const [open, setOpen] = useState(false)
  return (
    <Collapsible open={open} onOpenChange={setOpen} className="w-full">
      <CollapsibleTrigger render={<Button variant="ghost" size="xs" className="-ml-2" />}>
        {t(open ? "builds.hideDetails" : "builds.details")}
      </CollapsibleTrigger>
      <CollapsibleContent>
        <RunMessage message={message} />
      </CollapsibleContent>
    </Collapsible>
  )
}

/** The button that uploads a folder or archive to a build, with its drawer. */
export function UploadAction({
  project,
  build,
  primary,
}: {
  project: string
  build: string
  primary: boolean
}) {
  const t = useT()
  const gate = useHereGate("build")
  const [uploading, setUploading] = useState(false)
  return (
    <>
      <GateTip gate={gate}>
        <Button
          variant={primary ? "default" : "outline"}
          size="sm"
          onClick={() => setUploading(true)}
        >
          <UploadSimpleIcon aria-hidden="true" data-icon="inline-start" />
          {t("builds.upload")}
        </Button>
      </GateTip>
      {uploading ? (
        <UploadFolderDrawer project={project} build={build} onClose={() => setUploading(false)} />
      ) : null}
    </>
  )
}

/** Where the build takes its source from: the uploaded folder or the Git repository. */
function SourceFact({ build, branch }: { build: Build; branch: string | undefined }) {
  const t = useT()
  if (build.archive)
    return (
      <SummaryCardFact label={t("overview.source")}>
        <FolderSimpleIcon aria-hidden="true" />
        <span className="truncate">{t("builds.sourceUpload")}</span>
        {build.archiveDigest ? (
          <span className="truncate font-mono text-secondary" translate="no">
            {shortSha(build.archiveDigest)}
          </span>
        ) : null}
      </SummaryCardFact>
    )
  if (!build.gitUrl) return null
  return (
    <SummaryCardFact label={t("overview.source")}>
      <GitBranchIcon aria-hidden="true" />
      <span className="truncate font-mono" title={build.gitUrl} translate="no">
        {build.gitUrl.replace(/^https?:\/\//, "")}
        {branch ? `@${branch}` : ""}
      </span>
      {branch ? null : <span className="shrink-0 text-secondary">{t("builds.defaultBranch")}</span>}
    </SummaryCardFact>
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
  const toggleGate = useHereGate("changeServices")
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
          <GateTip gate={toggleGate}>{control}</GateTip>
        )}
      </div>
      {missing ? (
        <InstallAlert
          variant="info"
          text={t("builds.keelMissing")}
          components={[needed("image-updates", capabilities.data?.items)]}
        />
      ) : null}
    </section>
  )
}

const triggers = {
  webhook: "builds.trigger.webhook",
  poll: "builds.trigger.poll",
  manual: "builds.trigger.manual",
} as const

/** What started a run, how long it took and why it failed, in a line. */
function runMeta(t: ReturnType<typeof useT>, run: BuildRun, now: number) {
  const ms = durationOf(run, now)
  const trigger = run.trigger ? triggers[run.trigger as keyof typeof triggers] : undefined
  const parts = [
    trigger ? t(trigger) : undefined,
    ms === undefined
      ? undefined
      : t(run.finished ? "builds.took" : "builds.runningFor", Math.round(ms / 1000)),
  ]
  return parts.filter(Boolean).join(t("common.sep")) || undefined
}

function History({ project, runs }: { project: string; runs: BuildRun[] }) {
  const t = useT()
  const [now] = useState(() => Date.now())
  if (runs.length === 0) return null
  return (
    <section aria-labelledby="builds-history" className="flex flex-col gap-4">
      <SectionHeader id="builds-history" title={t("builds.history")} />
      <Timeline aria-labelledby="builds-history">
        {runs.map((run, i) => {
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
              meta={runMeta(t, run, now)}
            >
              <RunLogs project={project} run={run} newest={i === 0} />
            </TimelineItem>
          )
        })}
      </Timeline>
    </section>
  )
}

/** The run's phases and steps; the newest opens by itself while it runs or after it failed. */
function RunLogs({ project, run, newest }: { project: string; run: BuildRun; newest: boolean }) {
  const t = useT()
  const tone = runTone(run)
  return (
    <Collapsible defaultOpen={newest && (tone === "progress" || tone === "fail")}>
      <CollapsibleTrigger variant="disclosure">
        <CaretRightIcon aria-hidden="true" />
        {t("builds.logs")}
      </CollapsibleTrigger>
      <CollapsibleContent>
        {run.pod ? (
          <RunSteps project={project} pod={run.pod} phases={run.phases} />
        ) : (
          <p className="mt-2 text-sm text-secondary">
            {run.message ? (
              <span
                className="mb-2 block font-mono text-xs break-words whitespace-pre-wrap"
                lang="en"
                translate="no"
              >
                {run.message}
              </span>
            ) : null}
            {t("builds.logsGone")}
          </p>
        )}
      </CollapsibleContent>
    </Collapsible>
  )
}
