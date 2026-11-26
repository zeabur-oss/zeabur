import { useNavigate } from "@tanstack/react-router"
import { useState } from "react"

import {
  MethodField,
  MoreSection,
  ServiceNameField,
  Note,
  BuiltPort,
  BuiltPublicUrl,
  useBuiltForm,
} from "@/components/add/built-service"
import {
  AccessFields,
  AddDeployKey,
  RepositoryFields,
  useDeployKeyReveal,
  useGitSource,
  type DeployKeyReveal,
} from "@/components/add/git-source-fields"
import { repositoryUrl, useGitHubSource } from "@/components/add/repository-field"
import { PlanDrawer, ReviewFooter, ReviewForm } from "@/components/plan/plan-drawer"
import { usePlanStep, type PlanStepState } from "@/components/plan/planner"
import { useT } from "@/i18n"
import { serviceLink } from "@/lib/place"
import {
  createGitService,
  deployKeyPage,
  builtProblems,
  newGitService,
  parseRepository,
  type GitService,
} from "@/model/git-service"
import { builtName } from "@/model/image-service"
import { Button } from "@zeabur-oss/design-system/components/button"

/** "Deploy from Git": Zeabur Builder's Build, the service that runs what it builds, and the
 * repository's credentials, in one plan. */
export function GitDrawer({ project, onClose }: { project: string; onClose: () => void }) {
  const t = useT()
  const navigate = useNavigate()
  const github = useGitHubSource(project)
  const built = useBuiltForm<GitService>(project, newGitService, (f, taken) => {
    const repo = parseRepository(repositoryUrl(github, f.url))
    return builtName(repo?.path.split("/").at(-1) ?? "", taken)
  })
  const { form, set, shown, options, name } = built
  const git = useGitSource(github, form, set, options.secrets)
  const { repo, keyHost, access } = git
  // The live suggestion moves on (`site` → `site-2`) once the new service is listed.
  const [planned, setPlanned] = useState(name)

  const resolved: GitService = { ...built.resolved, ...git.resolved }
  const problems = {
    ...builtProblems(resolved, {
      taken: built.taken,
      badEnv: built.badEnv.length > 0,
      badBuildEnv: built.badBuildEnv.length > 0,
    }),
    ...git.problems,
  }
  const blocked = Object.values(problems).some(Boolean)

  const openService = () =>
    void navigate(serviceLink(project, { kind: "Deployment", name: planned }))
  const key = useKeyStage(project, planned, access === "deployKey", {
    onClose,
    openService,
  })
  const planner = usePlanStep(project, key.onDone, key.onApplied, key.onFailed)
  const review = () => {
    built.try()
    if (blocked) return
    setPlanned(name)
    planner.plan({
      title: t("add.planGit", name),
      changes: createGitService(resolved),
    })
  }

  if (key.stage)
    return (
      <KeyStageDrawer
        step={planner}
        applying={key.stage === "applying"}
        reveal={key.reveal}
        host={keyHost}
        page={repo && deployKeyPage(repo)}
        onOpen={openService}
        onClose={onClose}
      />
    )

  return (
    <PlanDrawer
      step={planner}
      onClose={onClose}
      title={t("add.gitTitle")}
      description={t("add.gitBody", project)}
      footer={<ReviewFooter onCancel={onClose} onReview={review} />}
    >
      <ReviewForm onReview={review}>
        <RepositoryFields
          git={git}
          shown={shown}
          aside={<MethodField built={built} source="git" />}
        />
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
          <ServiceNameField built={built} problems={problems} />
          <BuiltPort built={built} problems={problems} />
        </div>
        <BuiltPublicUrl built={built} problems={problems} />
        <AccessFields git={git} shown={shown} tried={built.tried} />
        <MoreSection built={built} problems={problems} source="git" />
        <Note>{t("add.gitAfter")}</Note>
      </ReviewForm>
    </PlanDrawer>
  )
}

/** The drawer once a deploy key's apply starts: the public key to add, then the service. */
function KeyStageDrawer({
  step,
  applying,
  reveal,
  host,
  page,
  onOpen,
  onClose,
}: {
  step: PlanStepState
  applying: boolean
  reveal: DeployKeyReveal
  host?: string
  page?: string
  onOpen: () => void
  onClose: () => void
}) {
  const t = useT()
  return (
    <PlanDrawer
      step={step}
      onClose={onClose}
      title={t("add.gitTitle")}
      footer={
        <div className="flex justify-end">
          <Button
            size="sm"
            disabled={applying}
            onClick={() => {
              onOpen()
              onClose()
            }}
          >
            {t("quickLook.open")}
          </Button>
        </div>
      }
    >
      <AddDeployKey applying={applying} reveal={reveal} host={host} page={page} />
    </PlanDrawer>
  )
}

/** With a deploy key, the drawer stays after the apply starts and shows the public key the apply
 * made for `build`; without one, it closes and the service page opens once applied. */
function useKeyStage(
  project: string,
  build: string,
  withKey: boolean,
  { onClose, openService }: { onClose: () => void; openService: () => void },
) {
  const [stage, setStage] = useState<"applying" | "applied">()
  const reveal = useDeployKeyReveal(project, build)
  return {
    stage,
    reveal,
    onDone: () => (withKey ? setStage("applying") : onClose()),
    onApplied: () => {
      if (!withKey) return openService()
      setStage("applied")
      reveal.mutate()
    },
    onFailed: () => setStage(undefined),
  }
}
