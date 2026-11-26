import { useState } from "react"

import { Note, PathFields, type Shown } from "@/components/add/built-service"
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
import { usePlanner } from "@/components/plan/planner"
import { useT } from "@/i18n"
import type { Build } from "@/model/builds"
import {
  buildMethods,
  deployKeyPage,
  gitSourceChange,
  newGitSource,
  pathProblem,
  type BuildMethod,
  type GitSource,
} from "@/model/git-service"
import type { Service } from "@/model/service"
import { Button } from "@zeabur-oss/design-system/components/button"
import { Select } from "@zeabur-oss/design-system/components/select"

/** Settings → Build of a service that builds from no repository: the repository form of "Deploy
 * from Git", sent as one `setBuild` change. An uploaded folder's Build reads it instead. */
export function GitSourceForm({
  project,
  service,
  upload,
  secrets,
  open,
  onOpenChange: setOpen,
}: {
  project: string
  service: Service
  upload?: Build
  secrets: boolean
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const t = useT()
  const choice = useGitChoice(project)
  const why = choice.gate.open && !choice.open ? gitEntry(choice, "git", t).why : undefined
  return (
    <div className="flex flex-col gap-4 rounded-xl border border-v2-border bg-card p-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex min-w-0 flex-col gap-0.5">
          <h3 className="text-sm font-medium">{t("gitSource.title")}</h3>
          <p className="type-meta text-muted-foreground">
            {why || t(upload ? "gitSource.uploadHint" : "gitSource.imageHint")}
          </p>
        </div>
        {open ? null : (
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={!choice.open}
            onClick={() => setOpen(true)}
          >
            {t("gitSource.open")}
          </Button>
        )}
      </div>
      {open ? (
        <Connect
          project={project}
          service={service}
          upload={upload}
          secrets={secrets}
          onClose={() => setOpen(false)}
        />
      ) : null}
    </div>
  )
}

function Connect({
  project,
  service,
  upload,
  secrets,
  onClose,
}: {
  project: string
  service: Service
  upload?: Build
  secrets: boolean
  onClose: () => void
}) {
  const t = useT()
  const github = useGitHubSource(project)
  const [form, setForm] = useState<GitSource>(() =>
    newGitSource(upload?.builder === "dockerfile" ? "dockerfile" : "railpack"),
  )
  const set = (patch: Partial<GitSource>) => setForm((f) => ({ ...f, ...patch }))
  const git = useGitSource(github, form, set, secrets)
  const [tried, setTried] = useState(false)
  const shown: Shown = (problem, text) => (tried && problem ? [text()] : [])
  const main = service.containers.filter((c) => !c.init)
  const [container, setContainer] = useState(main[0]?.name ?? "")
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
      aria-label={t("gitSource.title")}
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
      <PathFields
        form={form}
        onDirectory={(directory) => set({ directory })}
        onDockerfile={(dockerfile) => set({ dockerfile })}
        problems={problems}
        shown={shown}
      />
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
      <AccessFields git={git} shown={shown} tried={tried} />
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
