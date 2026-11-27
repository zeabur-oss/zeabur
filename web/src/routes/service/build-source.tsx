import { useState } from "react"
import FolderSimpleIcon from "~icons/ph/folder-simple"
import GitBranchIcon from "~icons/ph/git-branch"

import { Note, PathFields, useOfferedMethods, type Shown } from "@/components/add/built-service"
import { FolderDrop, useFolder } from "@/components/add/folder-drop"
import {
  AccessFields,
  AddDeployKey,
  type DeployKeyReveal,
  type GitSourceState,
  RepositoryFields,
  useDeployKeyReveal,
  useGitSource,
} from "@/components/add/git-source-fields"
import { gitEntry, useGitChoice } from "@/components/add/menu"
import { useGitHubSource } from "@/components/add/repository-field"
import { FormField } from "@/components/form-field"
import { PlanDrawer, ReviewFooter, ReviewForm } from "@/components/plan/plan-drawer"
import { usePlanner, usePlanStep } from "@/components/plan/planner"
import { useT } from "@/i18n"
import { branchOf, sourceContainer, type Build } from "@/model/builds"
import {
  deployKeyPage,
  gitSourceChange,
  gitSourceOf,
  withPathsOf,
  pathProblem,
  type BuildMethod,
  type GitSource,
} from "@/model/git-service"
import type { Service } from "@/model/service"
import { newRequest, startUpload } from "@/model/upload"
import { folderMethod, uploadSourceChange } from "@/model/upload-service"
import { UploadAction } from "@/routes/service/builds"
import { Button } from "@zeabur-oss/design-system/components/button"
import { Select } from "@zeabur-oss/design-system/components/select"

type Choice = "git" | "folder"

/** Settings → Build: what the service's Build reads, a Git repository or a folder from this
 * computer, changed with one `setBuild`. An uploaded folder's Build keeps its upload action. */
export function BuildSource({
  project,
  service,
  build,
  secrets,
}: {
  project: string
  service: Service
  /** The Build named after the service, if any. */
  build?: Build
  secrets: boolean
}) {
  const t = useT()
  const choice = useGitChoice(project)
  const why = choice.gate.open && !choice.open ? gitEntry(choice, "git", t).why : undefined
  // Stays open after the apply turns the service into a Git one, to show its deploy key.
  const [open, setOpen] = useState<Choice>()
  return (
    <div className="flex flex-col gap-4 rounded-xl border border-v2-border bg-card p-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <SourceSummary build={build} why={why} />
        {open === "git" ? null : (
          <SourceActions project={project} build={build} disabled={!choice.open} onOpen={setOpen} />
        )}
      </div>
      {open === "git" ? (
        <Connect
          project={project}
          service={service}
          build={build}
          secrets={secrets}
          onClose={() => setOpen(undefined)}
        />
      ) : null}
      {open === "folder" ? (
        <UploadSource
          project={project}
          service={service}
          build={build}
          onClose={() => setOpen(undefined)}
        />
      ) : null}
    </div>
  )
}

/** The card's sentence for each source. */
const sourceHint = (build: Build | undefined) =>
  build?.gitUrl ? "buildSource.git" : build?.archive ? "buildSource.upload" : "buildSource.image"

/** What the Build reads now: its repository and branch, and what a change does. */
function SourceSummary({ build, why }: { build?: Build; why?: string }) {
  const t = useT()
  const branch = build ? branchOf(build) : undefined
  return (
    <div className="flex min-w-0 flex-col gap-0.5">
      <h3 className="text-sm font-medium">{t("buildSource.title")}</h3>
      {build?.gitUrl ? (
        <p className="flex min-w-0 items-center gap-3 font-mono text-sm" translate="no">
          <span className="truncate">{build.gitUrl}</span>
          {branch ? (
            <span className="inline-flex shrink-0 items-center gap-1">
              <GitBranchIcon aria-hidden="true" className="size-3.5" />
              {branch}
            </span>
          ) : null}
        </p>
      ) : null}
      <p className="type-meta text-muted-foreground">{why || t(sourceHint(build))}</p>
    </div>
  )
}

/** Git, and a folder: a new one for an uploaded folder's Build, else a switch to one. */
function SourceActions({
  project,
  build,
  disabled,
  onOpen,
}: {
  project: string
  build?: Build
  disabled: boolean
  onOpen: (choice: Choice) => void
}) {
  const t = useT()
  return (
    <div className="flex flex-wrap gap-2">
      <Button
        type="button"
        variant="outline"
        size="sm"
        disabled={disabled}
        onClick={() => onOpen("git")}
      >
        <GitBranchIcon aria-hidden="true" data-icon="inline-start" />
        {t(build?.gitUrl ? "buildSource.changeGit" : "gitSource.open")}
      </Button>
      {build?.archive ? (
        <UploadAction project={project} build={build.name} primary={false} />
      ) : (
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={disabled}
          onClick={() => onOpen("folder")}
        >
          <FolderSimpleIcon aria-hidden="true" data-icon="inline-start" />
          {t("builds.upload")}
        </Button>
      )}
    </div>
  )
}

/** After a deploy key's apply: its public key to add on the host. */
function KeyAdded({
  reveal,
  git,
  onClose,
}: {
  reveal: DeployKeyReveal
  git: GitSourceState
  onClose: () => void
}) {
  const t = useT()
  return (
    <>
      <AddDeployKey
        applying={false}
        reveal={reveal}
        host={git.keyHost}
        page={git.repo && deployKeyPage(git.repo)}
      />
      <div className="flex justify-end">
        <Button type="button" size="sm" onClick={onClose}>
          {t("gitSource.done")}
        </Button>
      </div>
    </>
  )
}

/** Which container runs the build, when the service has several. */
function ContainerField({
  service,
  value,
  onChange,
}: {
  service: Service
  value: string
  onChange: (container: string) => void
}) {
  const t = useT()
  const main = service.containers.filter((c) => !c.init)
  if (main.length < 2) return null
  return (
    <FormField label={t("gitSource.container")}>
      <Select
        size="sm"
        value={value}
        onValueChange={(next) => onChange(next as string)}
        items={main.map((c) => ({ value: c.name, label: c.name }))}
      />
    </FormField>
  )
}

/** How the source is built: Railpack, its Dockerfile or the agent. */
function MethodSelect({
  project,
  value,
  onChange,
  source = "add",
}: {
  project: string
  value: BuildMethod
  onChange: (method: BuildMethod) => void
  /** `upload` says "folder" where `add` says "repository". */
  source?: "add" | "upload"
}) {
  const t = useT()
  const methods = useOfferedMethods(project, value)
  return (
    <FormField label={t("add.field.method")} hint={t(`${source}.methodHint.${value}`)}>
      <Select
        size="sm"
        value={value}
        onValueChange={(method) => onChange(method as BuildMethod)}
        items={methods.map((m) => ({ value: m, label: t(`add.method.${m}`) }))}
      />
    </FormField>
  )
}

function Connect({
  project,
  service,
  build,
  secrets,
  onClose,
}: {
  project: string
  service: Service
  build?: Build
  secrets: boolean
  onClose: () => void
}) {
  const t = useT()
  const [current] = useState(() => gitSourceOf(build))
  const github = useGitHubSource(project, current.url)
  const [form, setForm] = useState<GitSource>(current)
  const set = (patch: Partial<GitSource>) => setForm((f) => ({ ...f, ...patch }))
  const git = useGitSource(github, form, set, secrets)
  const [tried, setTried] = useState(false)
  const shown: Shown = (problem, text) => (tried && problem ? [text()] : [])
  const [container, setContainer] = useState(sourceContainer(service, build) ?? "")
  const problems = {
    ...git.problems,
    directory: pathProblem(form.directory),
    dockerfile: pathProblem(form.dockerfile),
  }
  const withKey = git.access === "deployKey"
  const reveal = useDeployKeyReveal(project, service.name)
  const [applied, setApplied] = useState(false)
  const planner = usePlanner(
    project,
    () => {
      if (!withKey) return onClose()
      setApplied(true)
      reveal.mutate()
    },
    true,
  )
  const review = () => {
    setTried(true)
    if (Object.values(problems).some(Boolean)) return
    planner.plan({
      title: t("title.gitSource", service.name),
      changes: [
        gitSourceChange(
          withPathsOf({ ...form, ...git.resolved }, build),
          { kind: service.kind, name: service.name },
          container,
        ),
      ],
    })
  }

  if (applied) return <KeyAdded reveal={reveal} git={git} onClose={onClose} />

  return (
    <ReviewForm onReview={review}>
      <RepositoryFields
        git={git}
        shown={shown}
        aside={
          <MethodSelect
            project={project}
            value={form.method}
            onChange={(method) => set({ method })}
          />
        }
      />
      {/* A Build's paths stay as its options above set them. */}
      {build ? null : (
        <PathFields
          form={form}
          onDirectory={(directory) => set({ directory })}
          onDockerfile={(dockerfile) => set({ dockerfile })}
          problems={problems}
          shown={shown}
        />
      )}
      <ContainerField service={service} value={container} onChange={setContainer} />
      <AccessFields git={git} shown={shown} tried={tried} savedToken={current.access === "token"} />
      <Note>{t("add.gitAfter")}</Note>
      <div className="flex justify-end gap-2">
        <Button type="button" variant="ghost" size="sm" onClick={onClose}>
          {t("plan.cancel")}
        </Button>
        <Button type="submit" size="sm">
          {t("edit.review")}
        </Button>
      </div>
      {planner.dialog}
    </ReviewForm>
  )
}

/** "Upload a folder" for a service that builds no folder: its Build reads the folder instead, or
 * a new one builds it, and the folder goes up once the plan is applied. */
function UploadSource({
  project,
  service,
  build,
  onClose,
}: {
  project: string
  service: Service
  build?: Build
  onClose: () => void
}) {
  const t = useT()
  const folder = useFolder()
  const [request] = useState(newRequest)
  const [tried, setTried] = useState(false)
  const ready = folder.state.phase === "ready" ? folder.state : undefined
  const [chosen, setChosen] = useState<BuildMethod>()
  const method = chosen ?? folderMethod(build, (path) => ready?.dockerfiles.has(path) ?? false)
  const planner = usePlanStep(project, onClose, () => {
    if (ready)
      void startUpload({ project, build: service.name, blob: ready.packed.blob, request, t })
  })
  const review = () => {
    setTried(true)
    if (!ready) return
    planner.plan({
      title: t("title.uploadSource", service.name),
      changes: [
        uploadSourceChange(build, {
          workload: { kind: service.kind, name: service.name },
          container: sourceContainer(service, build),
          request,
          method,
        }),
      ],
    })
  }
  return (
    <PlanDrawer
      step={planner}
      onClose={onClose}
      title={t("builds.upload")}
      description={t("buildSource.uploadBody", service.name)}
      footer={<ReviewFooter onCancel={onClose} onReview={review} />}
    >
      <ReviewForm onReview={review}>
        <FolderDrop folder={folder} missing={tried && !ready} />
        <MethodSelect project={project} value={method} onChange={setChosen} source="upload" />
        <Note>{t("upload.after")}</Note>
      </ReviewForm>
    </PlanDrawer>
  )
}
