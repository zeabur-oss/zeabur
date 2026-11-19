import { useMutation, type UseMutationResult } from "@tanstack/react-query"
import { useNavigate } from "@tanstack/react-router"
import { useState } from "react"
import ArrowSquareOutIcon from "~icons/ph/arrow-square-out"

import { toProblem, type Schemas } from "@/api/client"
import * as zeabur from "@/api/zeabur"
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
  appRepository,
  GitHubAccess,
  RepositoryField,
  repositoryUrl,
  useGitHubSource,
} from "@/components/add/repository-field"
import { FormField } from "@/components/form-field"
import { LoadingRows } from "@/components/page"
import { PlanDrawer, ReviewFooter, ReviewForm } from "@/components/plan/plan-drawer"
import { usePlanStep, type PlanStepState } from "@/components/plan/planner"
import { useT } from "@/i18n"
import { serviceLink } from "@/lib/place"
import {
  createGitService,
  deployKeyHost,
  deployKeyPage,
  gitProblems,
  keyscanCommand,
  newGitService,
  parseRepository,
  pinnedHost,
  tokenUser,
  type Access,
  type GitService,
} from "@/model/git-service"
import { builtName } from "@/model/image-service"
import { Alert, AlertDescription } from "@zeabur-oss/design-system/components/alert"
import { Button } from "@zeabur-oss/design-system/components/button"
import { CopyInput } from "@zeabur-oss/design-system/components/copy-input"
import { Input } from "@zeabur-oss/design-system/components/input"
import { RadioGroup, RadioOption } from "@zeabur-oss/design-system/components/radio"
import { Textarea } from "@zeabur-oss/design-system/components/textarea"

interface CheckedHost {
  host: string
  knownHosts: string
  hostKeys: { algorithm: string; fingerprint: string }[]
}

/** "Deploy from Git": Zeabur Builder's Build, the service that runs what it builds, and the
 * repository's credentials, in one plan. */
export function GitDrawer({ project, onClose }: { project: string; onClose: () => void }) {
  const t = useT()
  const navigate = useNavigate()
  const github = useGitHubSource(project)
  const fromGitHub = github.source === "github"
  const built = useBuiltForm<GitService>(project, newGitService, (f, taken) => {
    const repo = parseRepository(repositoryUrl(github, f.url))
    return builtName(repo?.path.split("/").at(-1) ?? "", taken)
  })
  const { form, set, shown, options, name } = built
  const [userSet, setUserSet] = useState(false)
  const [checked, setChecked] = useState<CheckedHost>()
  // The live suggestion moves on (`site` → `site-2`) once the new service is listed.
  const [planned, setPlanned] = useState(name)

  const url = repositoryUrl(github, form.url)
  const repo = parseRepository(url)
  const keyHost = deployKeyHost(url)
  const secrets = options.secrets
  const readByApp = appRepository(github, form.url)
  const access = accessOf(form.access, {
    fromGitHub,
    readByApp: readByApp !== undefined,
    secrets,
    keyHost,
  })
  const username = userSet ? form.username : tokenUser(repo?.host)
  const resolved: GitService = {
    ...built.resolved,
    url,
    access,
    installationId: readByApp?.installationId,
    username,
  }
  const problems = gitProblems(resolved, {
    taken: built.taken,
    badEnv: built.badEnv.length > 0,
    badBuildEnv: built.badBuildEnv.length > 0,
    hostChecked: hostChecked(checked, keyHost, form.knownHosts),
  })
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
        <RepositoryField
          github={github}
          url={form.url}
          onUrl={(next) => set({ url: next })}
          errors={shown(problems.url || problems.installation, () =>
            fromGitHub
              ? t("add.problem.githubRepository")
              : t(`add.problem.repository.${problems.url ?? "empty"}`),
          )}
        />
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
          <FormField
            label={t("add.field.branch")}
            hint={t("add.branchHint")}
            errors={shown(problems.branch, () => t("add.problem.branch"))}
          >
            <Input
              size="sm"
              mono
              value={form.branch}
              placeholder={readByApp?.defaultBranch ?? t("add.branchDefault")}
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => set({ branch: event.target.value })}
            />
          </FormField>
          <MethodField built={built} source="git" />
        </div>
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
          <ServiceNameField built={built} problems={problems} />
          <BuiltPort built={built} problems={problems} />
        </div>
        <BuiltPublicUrl built={built} problems={problems} />
        <fieldset className="flex min-w-0 flex-col gap-2">
          <legend className="mb-1.5 text-sm font-medium">{t("add.access")}</legend>
          <AccessChoice
            access={access}
            listedAt={github.listedAt}
            secrets={secrets}
            keyHost={keyHost}
            onAccess={(next) => set({ access: next })}
          />
          {access === "deployKey" && keyHost ? (
            <DeployKeyFields
              project={project}
              host={keyHost}
              knownHosts={form.knownHosts}
              onKnownHosts={(knownHosts) => set({ knownHosts })}
              checked={checked}
              missing={built.tried && !!problems.hostKeys}
              onChecked={setChecked}
            />
          ) : null}
          {access === "token" ? (
            <TokenFields
              username={username}
              onUsername={(next) => {
                setUserSet(true)
                set({ username: next })
              }}
              token={form.token}
              onToken={(token) => set({ token })}
              errors={shown(problems.token, () => t("add.problem.token"))}
            />
          ) : null}
        </fieldset>
        <MoreSection built={built} problems={problems} source="git" />
        <Note>{t("add.gitAfter")}</Note>
      </ReviewForm>
    </PlanDrawer>
  )
}

/** No deploy key, a host Zeabur pins, or keys checked for these very lines. */
const hostChecked = (checked: CheckedHost | undefined, host: string | undefined, lines: string) =>
  !host || pinnedHost(host) || (checked?.host === host && checked.knownHosts === lines)

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
  reveal: UseMutationResult<Schemas.DeployKeyPublic, Error, void>
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

function AccessChoice({
  access,
  listedAt,
  secrets,
  keyHost,
  onAccess,
}: {
  access: Access
  listedAt?: string
  secrets: boolean
  keyHost?: string
  onAccess: (access: Access) => void
}) {
  const t = useT()
  if (access === "githubApp") return <GitHubAccess listedAt={listedAt} />
  if (!secrets) return <Note>{t("add.access.noSecrets")}</Note>
  return (
    <RadioGroup value={access} onValueChange={(value) => onAccess(value as Access)}>
      <RadioOption value="public" label={t("add.access.public")} />
      <RadioOption
        value="deployKey"
        disabled={!keyHost}
        label={t("add.access.deployKey")}
        description={t("add.access.deployKeyHint")}
      />
      <RadioOption
        value="token"
        label={t("add.access.token")}
        description={t("add.access.tokenHint")}
      />
    </RadioGroup>
  )
}

function TokenFields({
  username,
  onUsername,
  token,
  onToken,
  errors,
}: {
  username: string
  onUsername: (value: string) => void
  token: string
  onToken: (value: string) => void
  errors: string[]
}) {
  const t = useT()
  return (
    <div className="grid grid-cols-1 gap-4 pt-2 sm:grid-cols-2">
      <FormField label={t("add.registryUser")}>
        <Input
          size="sm"
          value={username}
          autoComplete="off"
          onChange={(event) => onUsername(event.target.value)}
        />
      </FormField>
      <FormField label={t("add.token")} errors={errors}>
        <Input
          size="sm"
          type="password"
          value={token}
          autoComplete="new-password"
          onChange={(event) => onToken(event.target.value)}
        />
      </FormField>
    </div>
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
  const reveal = useMutation({
    mutationFn: async () => await zeabur.revealDeployKey(project, build),
  })
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

/** The access the form builds with: the App for a GitHub repository it can read; otherwise what
 * the person chose, as far as their rights and the host allow. */
function accessOf(
  chosen: Access,
  {
    fromGitHub,
    readByApp,
    secrets,
    keyHost,
  }: { fromGitHub: boolean; readByApp: boolean; secrets: boolean; keyHost?: string },
): Access {
  if (fromGitHub || readByApp) return "githubApp"
  if (!secrets || chosen === "githubApp") return "public"
  return chosen === "deployKey" && !keyHost ? "public" : chosen
}

/** A deploy key for the repository's host, made when the service is created. A host whose key
 * Zeabur does not pin takes the person's `known_hosts` lines, and they check its fingerprints
 * first. */
function DeployKeyFields({
  project,
  host,
  knownHosts,
  onKnownHosts,
  checked,
  missing,
  onChecked,
}: {
  project: string
  host: string
  knownHosts: string
  onKnownHosts: (value: string) => void
  checked?: CheckedHost
  missing: boolean
  onChecked: (checked: CheckedHost) => void
}) {
  const t = useT()
  const check = useMutation({
    mutationFn: async (lines: string) =>
      await zeabur.inspectDeployKeyHost(project, { host, knownHosts: lines }),
    onSuccess: ({ hostKeys }, lines) => onChecked({ host, knownHosts: lines, hostKeys }),
  })
  if (pinnedHost(host))
    return <p className="pt-2 type-meta text-muted-foreground">{t("add.deployKeyLater")}</p>
  return (
    <div className="flex flex-col gap-2 pt-2">
      <div className="flex flex-col gap-1.5">
        <KnownHostsFields host={host} value={knownHosts} onChange={onKnownHosts} />
        {checked && hostChecked(checked, host, knownHosts) ? (
          <HostKeys hostKeys={checked.hostKeys} />
        ) : (
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="self-start"
            disabled={check.isPending || knownHosts.trim() === ""}
            onClick={() => check.mutate(knownHosts)}
          >
            {t("add.hostKeysRead")}
          </Button>
        )}
        {missing ? (
          <p className="text-xs leading-4 text-destructive">{t("add.problem.deployKey")}</p>
        ) : null}
      </div>
      <p className="type-meta text-muted-foreground">{t("add.deployKeyLater")}</p>
      {check.isError ? (
        <Alert variant="destructive" role="alert">
          <AlertDescription>{toProblem(check.error).detail}</AlertDescription>
        </Alert>
      ) : null}
    </div>
  )
}

/** The command that prints the host's keys, and where its output goes. */
function KnownHostsFields({
  host,
  value,
  onChange,
}: {
  host: string
  value: string
  onChange: (value: string) => void
}) {
  const t = useT()
  return (
    <div className="flex flex-col gap-4 pb-1.5">
      <FormField label={t("add.keyscan")} hint={t("add.keyscanHint")}>
        <CopyInput value={keyscanCommand(host)} copyLabel={t("common.copy")} />
      </FormField>
      <FormField label={t("add.knownHosts")} hint={t("add.knownHostsHint")}>
        <Textarea
          mono
          rows={3}
          value={value}
          spellCheck={false}
          onChange={(event) => onChange(event.target.value)}
        />
      </FormField>
    </div>
  )
}

/** The host keys the Secret will trust, to compare with the host's published fingerprints. */
function HostKeys({ hostKeys }: { hostKeys: CheckedHost["hostKeys"] }) {
  const t = useT()
  return (
    <div className="flex flex-col gap-1">
      <p className="text-sm font-medium">{t("add.hostKeys")}</p>
      <ul className="flex flex-col gap-0.5 font-mono text-xs break-all">
        {hostKeys.map((k) => (
          <li key={k.fingerprint}>
            {k.algorithm} {k.fingerprint}
          </li>
        ))}
      </ul>
      <p className="type-meta text-muted-foreground">{t("add.hostKeysCheck")}</p>
    </div>
  )
}

/** After the apply: the public key Zeabur made, read from its Secret, to add on the host. */
function AddDeployKey({
  applying,
  reveal,
  host,
  page,
}: {
  applying: boolean
  reveal: UseMutationResult<Schemas.DeployKeyPublic, Error, void>
  host?: string
  page?: string
}) {
  const t = useT()
  if (applying) return <LoadingRows label={t("add.deployKeyMaking")} rows={1} />
  return (
    <div className="flex flex-col gap-4">
      <p className="text-sm">{t("add.deployKeyMade")}</p>
      {reveal.data ? (
        <FormField label={t("add.deployKeyPublic")}>
          <CopyInput value={reveal.data.publicKey} copyLabel={t("common.copy")} />
        </FormField>
      ) : reveal.isError ? (
        <Alert variant="destructive" role="alert">
          <AlertDescription>{toProblem(reveal.error).detail}</AlertDescription>
        </Alert>
      ) : (
        <LoadingRows label={t("add.deployKeyPublic")} rows={1} />
      )}
      <p className="flex flex-wrap items-baseline gap-x-2 type-meta text-muted-foreground">
        {t("add.deployKeyAdd")}
        {page ? (
          <a
            href={page}
            target="_blank"
            rel="noreferrer"
            className="inline-flex items-center gap-1 text-foreground hover:underline"
          >
            {t("add.deployKeyOpen", host === "github.com" ? "GitHub" : "GitLab")}
            <ArrowSquareOutIcon aria-hidden="true" className="size-3.5" />
          </a>
        ) : null}
      </p>
    </div>
  )
}
