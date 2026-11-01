import { useMutation } from "@tanstack/react-query"
import { useNavigate } from "@tanstack/react-router"
import { useState } from "react"
import ArrowSquareOutIcon from "~icons/ph/arrow-square-out"

import { toProblem } from "@/api/client"
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
import { PlanDrawer, ReviewFooter, ReviewForm } from "@/components/plan/plan-drawer"
import { usePlanStep } from "@/components/plan/planner"
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
import { nameFromImage } from "@/model/image-service"
import { Alert, AlertDescription } from "@zeabur-oss/design-system/components/alert"
import { Button } from "@zeabur-oss/design-system/components/button"
import { CopyInput } from "@zeabur-oss/design-system/components/copy-input"
import { Input } from "@zeabur-oss/design-system/components/input"
import { RadioGroup, RadioOption } from "@zeabur-oss/design-system/components/radio"
import { Textarea } from "@zeabur-oss/design-system/components/textarea"

interface DeployKey {
  host: string
  publicKey: string
  key: string
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
    return repo ? nameFromImage(repo.path.split("/").at(-1) ?? "", taken) : ""
  })
  const { form, set, shown, options, name, domains, registry } = built
  const [userSet, setUserSet] = useState(false)
  const [deployKey, setDeployKey] = useState<DeployKey>()

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
  const key = deployKey && deployKey.host === keyHost ? deployKey : undefined
  const resolved: GitService = {
    ...built.resolved,
    url,
    access,
    installationId: readByApp?.installationId,
    username,
    sealedKey: key?.key,
  }
  const problems = gitProblems(resolved, {
    taken: built.taken,
    badEnv: built.badEnv.length > 0,
    badBuildEnv: built.badBuildEnv.length > 0,
    hasKey: !!key,
  })
  const blocked = Object.values(problems).some(Boolean)

  const planner = usePlanStep(project, onClose, () => {
    void navigate(serviceLink(project, { kind: "Deployment", name }))
  })
  const review = () => {
    built.try()
    if (blocked) return
    planner.plan({
      title: t("add.planGit", name),
      changes: createGitService(resolved, project, registry, domains),
    })
  }

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
          {access === "githubApp" ? (
            <GitHubAccess listedAt={github.listedAt} />
          ) : secrets ? (
            <RadioGroup value={access} onValueChange={(value) => set({ access: value as Access })}>
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
          ) : (
            <Note>{t("add.access.noSecrets")}</Note>
          )}
          {access === "deployKey" && keyHost ? (
            <DeployKeyFields
              project={project}
              host={keyHost}
              pinned={pinnedHost(keyHost)}
              page={repo ? deployKeyPage(repo) : undefined}
              made={key}
              missing={built.tried && !!problems.key}
              onMade={setDeployKey}
            />
          ) : null}
          {access === "token" ? (
            <div className="grid grid-cols-1 gap-4 pt-2 sm:grid-cols-2">
              <FormField label={t("add.registryUser")}>
                <Input
                  size="sm"
                  value={username}
                  autoComplete="off"
                  onChange={(event) => {
                    setUserSet(true)
                    set({ username: event.target.value })
                  }}
                />
              </FormField>
              <FormField
                label={t("add.token")}
                errors={shown(problems.token, () => t("add.problem.token"))}
              >
                <Input
                  size="sm"
                  type="password"
                  value={form.token}
                  autoComplete="new-password"
                  onChange={(event) => set({ token: event.target.value })}
                />
              </FormField>
            </div>
          ) : null}
        </fieldset>
        <MoreSection built={built} problems={problems} source="git" />
        <Note>{t("add.gitAfter")}</Note>
      </ReviewForm>
    </PlanDrawer>
  )
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

/** A deploy key for the repository's host: made on the server, its public half to add on the
 * host. A host whose key Zeabur does not pin takes the person's `known_hosts` lines first. */
function DeployKeyFields({
  project,
  host,
  pinned,
  page,
  made,
  missing,
  onMade,
}: {
  project: string
  host: string
  pinned: boolean
  page?: string
  made?: DeployKey
  missing: boolean
  onMade: (key: DeployKey) => void
}) {
  const t = useT()
  const [knownHosts, setKnownHosts] = useState("")
  const makeKey = useMutation({
    mutationFn: async (gitHost: string) =>
      await zeabur.createDeployKey(project, { host: gitHost, ...(pinned ? {} : { knownHosts }) }),
    onSuccess: (key, gitHost) => onMade({ host: gitHost, ...key }),
  })
  return (
    <div className="flex flex-col gap-2 pt-2">
      {made ? (
        <MadeDeployKey made={made} host={host} pinned={pinned} page={page} />
      ) : (
        <div className="flex flex-col gap-1.5">
          {pinned ? null : (
            <KnownHostsFields host={host} value={knownHosts} onChange={setKnownHosts} />
          )}
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="self-start"
            disabled={makeKey.isPending || (!pinned && knownHosts.trim() === "")}
            onClick={() => makeKey.mutate(host)}
          >
            {t("add.deployKeyMake")}
          </Button>
          {missing ? (
            <p className="text-xs leading-4 text-destructive">{t("add.problem.deployKey")}</p>
          ) : null}
        </div>
      )}
      {makeKey.isError ? (
        <Alert variant="destructive">
          <AlertDescription>{toProblem(makeKey.error).detail}</AlertDescription>
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

/** The public key to add on the host and, for a host Zeabur does not pin, the host keys to check. */
function MadeDeployKey({
  made,
  host,
  pinned,
  page,
}: {
  made: DeployKey
  host: string
  pinned: boolean
  page?: string
}) {
  const t = useT()
  return (
    <>
      <FormField label={t("add.deployKeyPublic")}>
        <CopyInput value={made.publicKey} copyLabel={t("common.copy")} />
      </FormField>
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
      {pinned ? null : (
        <div className="flex flex-col gap-1">
          <p className="text-sm font-medium">{t("add.hostKeys")}</p>
          <ul className="flex flex-col gap-0.5 font-mono text-xs break-all">
            {made.hostKeys.map((k) => (
              <li key={k.fingerprint}>
                {k.algorithm} {k.fingerprint}
              </li>
            ))}
          </ul>
          <p className="type-meta text-muted-foreground">{t("add.hostKeysCheck")}</p>
        </div>
      )}
    </>
  )
}
