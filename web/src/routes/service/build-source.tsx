import { useState } from "react"
import FolderSimpleIcon from "~icons/ph/folder-simple"
import GitBranchIcon from "~icons/ph/git-branch"

import { Note, PathFields, type Shown } from "@/components/add/built-service"
import { FolderDrop, useFolder } from "@/components/add/folder-drop"
import {
  AccessFields,
  AddDeployKey,
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
  buildMethods,
  deployKeyPage,
  gitSourceChange,
  gitSourceOf,
  pathProblem,
  type BuildMethod,
  type GitSource,
} from "@/model/git-service"
import type { Service } from "@/model/service"
import { newRequest, startUpload } from "@/model/upload"
import { uploadSourceChange } from "@/model/upload-service"
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
  const git = !!build?.gitUrl
  const branch = build ? branchOf(build) : undefined
  return (
    <div className="flex flex-col gap-4 rounded-xl border border-v2-border bg-card p-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex min-w-0 flex-col gap-0.5">
          <h3 className="text-sm font-medium">{t("buildSource.title")}</h3>
          {git ? (
            <p className="truncate font-mono text-sm" translate="no">
              {build?.gitUrl}
              {branch ? ` · ${branch}` : null}
            </p>
          ) : null}
          <p className="type-meta text-muted-foreground">
            {why ||
              t(
                git
                  ? "buildSource.git"
                  : build?.archive
                    ? "buildSource.upload"
                    : "buildSource.image",
              )}
          </p>
        </div>
        {open === "git" ? null : (
          <div className="flex flex-wrap gap-2">
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={!choice.open}
              onClick={() => setOpen("git")}
            >
              <GitBranchIcon aria-hidden="true" data-icon="inline-start" />
              {t(git ? "buildSource.changeGit" : "gitSource.open")}
            </Button>
            {build?.archive ? (
              <UploadAction project={project} build={build.name} primary={false} />
            ) : (
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={!choice.open}
                onClick={() => setOpen("folder")}
              >
                <FolderSimpleIcon aria-hidden="true" data-icon="inline-start" />
                {t("builds.upload")}
              </Button>
            )}
          </div>
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
  const main = service.containers.filter((c) => !c.init)
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
          { ...form, ...git.resolved },
          { kind: service.kind, name: service.name },
          container,
        ),
      ],
    })
  }

  if (applied)
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

  return (
    <form
      aria-label={t("buildSource.title")}
      className="flex flex-col gap-4"
      onSubmit={(event) => {
        event.preventDefault()
        review()
      }}
    >
      <RepositoryFields
        git={git}
        shown={shown}
        aside={
          <FormField label={t("add.field.method")} hint={t(`add.methodHint.${form.method}`)}>
            <Select
              size="sm"
              value={form.method}
              onValueChange={(method) => set({ method: method as BuildMethod })}
              items={buildMethods.map((m) => ({ value: m, label: t(`add.method.${m}`) }))}
            />
          </FormField>
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
      {main.length > 1 ? (
        <FormField label={t("gitSource.container")}>
          <Select
            size="sm"
            value={container}
            onValueChange={(value) => setContainer(value as string)}
            items={main.map((c) => ({ value: c.name, label: c.name }))}
          />
        </FormField>
      ) : null}
      <AccessFields
        git={git}
        shown={shown}
        tried={tried}
        tokenHint={current.access === "token" ? t("gitSource.tokenAgain") : undefined}
      />
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
    </form>
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
        uploadSourceChange(
          build,
          {
            workload: { kind: service.kind, name: service.name },
            container: sourceContainer(service, build),
            request,
          },
          (path) => ready.dockerfiles.has(path),
        ),
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
        <Note>{t("upload.after")}</Note>
      </ReviewForm>
    </PlanDrawer>
  )
}
