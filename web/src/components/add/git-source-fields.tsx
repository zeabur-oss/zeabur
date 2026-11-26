// How a Build reads a Git repository, as "Deploy from Git" and a service's Settings → Build both
// ask for it: the repository, its branch, and the access with its deploy key or token.
import { useMutation, type UseMutationResult } from "@tanstack/react-query"
import { useState, type ReactNode } from "react"
import ArrowSquareOutIcon from "~icons/ph/arrow-square-out"

import { toProblem, type Schemas } from "@/api/client"
import * as zeabur from "@/api/zeabur"
import { Note, type Shown } from "@/components/add/built-service"
import {
  appRepository,
  GitHubAccess,
  RepositoryField,
  repositoryUrl,
  type GitHubSource,
} from "@/components/add/repository-field"
import { FormField } from "@/components/form-field"
import { LoadingRows } from "@/components/page"
import { useT } from "@/i18n"
import {
  deployKeyHost,
  keyscanCommand,
  parseRepository,
  pinnedHost,
  repositoryProblems,
  tokenUser,
  type Access,
  type GitSource,
} from "@/model/git-service"
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

/** The repository part of a form: what the person picked or typed, resolved to the URL, access
 * and user the change sends, and what is wrong with it. */
export function useGitSource<F extends GitSource>(
  github: GitHubSource,
  form: F,
  set: (patch: Partial<F>) => void,
  secrets: boolean,
) {
  const [userSet, setUserSet] = useState(false)
  const [checked, setChecked] = useState<CheckedHost>()
  const url = repositoryUrl(github, form.url)
  const repo = parseRepository(url)
  const keyHost = deployKeyHost(url)
  const readByApp = appRepository(github, form.url)
  const access = accessOf(form.access, {
    fromGitHub: github.source === "github",
    readByApp: readByApp !== undefined,
    secrets,
    keyHost,
  })
  const username = userSet ? form.username : tokenUser(repo?.host)
  const resolved = { url, access, installationId: readByApp?.installationId, username }
  return {
    project: github.project,
    form,
    set,
    github,
    repo,
    keyHost,
    readByApp,
    access,
    username,
    secrets,
    checked,
    setChecked,
    typeUsername: (next: string) => {
      setUserSet(true)
      set({ username: next } as Partial<F>)
    },
    resolved,
    problems: repositoryProblems(
      { ...form, ...resolved },
      hostChecked(checked, keyHost, form.knownHosts),
    ),
  }
}

export type GitSourceState = ReturnType<typeof useGitSource<GitSource>>

/** The repository, then its branch beside `aside` (the build method). */
export function RepositoryFields({
  git,
  shown,
  aside,
}: {
  git: GitSourceState
  shown: Shown
  aside: ReactNode
}) {
  const t = useT()
  const { github, form, set, problems } = git
  return (
    <>
      <RepositoryField
        github={github}
        url={form.url}
        onUrl={(next) => set({ url: next })}
        errors={shown(problems.url || problems.installation, () =>
          github.source === "github"
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
            placeholder={git.readByApp?.defaultBranch ?? t("add.branchDefault")}
            autoComplete="off"
            spellCheck={false}
            onChange={(event) => set({ branch: event.target.value })}
          />
        </FormField>
        {aside}
      </div>
    </>
  )
}

/** How Zeabur Builder reads the repository: public, a deploy key, a token, or the GitHub App. */
export function AccessFields({
  git,
  shown,
  tried,
  savedToken = false,
}: {
  git: GitSourceState
  shown: Shown
  tried: boolean
  /** The Build reads with a token already, which Zeabur does not show. */
  savedToken?: boolean
}) {
  const t = useT()
  const { access, keyHost, form, set } = git
  return (
    <fieldset className="flex min-w-0 flex-col gap-2">
      <legend className="mb-1.5 text-sm font-medium">{t("add.access")}</legend>
      <AccessChoice
        access={access}
        listedAt={git.github.listedAt}
        secrets={git.secrets}
        keyHost={keyHost}
        onAccess={(next) => set({ access: next })}
      />
      {access === "deployKey" && keyHost ? (
        <DeployKeyFields
          project={git.project}
          host={keyHost}
          knownHosts={form.knownHosts}
          onKnownHosts={(knownHosts) => set({ knownHosts })}
          checked={git.checked}
          missing={tried && git.problems.hostKeys}
          onChecked={git.setChecked}
        />
      ) : null}
      {access === "token" ? (
        <TokenFields
          username={git.username}
          onUsername={git.typeUsername}
          token={form.token}
          onToken={(token) => set({ token })}
          hint={savedToken ? t("gitSource.tokenAgain") : undefined}
          errors={shown(git.problems.token, () => t("add.problem.token"))}
        />
      ) : null}
    </fieldset>
  )
}

/** The public key of `build`'s deploy key, read once its apply made it. */
export function useDeployKeyReveal(project: string, build: string) {
  return useMutation({
    mutationFn: async () => await zeabur.revealDeployKey(project, build),
  })
}

export type DeployKeyReveal = UseMutationResult<Schemas.DeployKeyPublic, Error, void>

/** No deploy key, a host Zeabur pins, or keys checked for these very lines. */
const hostChecked = (checked: CheckedHost | undefined, host: string | undefined, lines: string) =>
  !host || pinnedHost(host) || (checked?.host === host && checked.knownHosts === lines)

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
  hint,
  errors,
}: {
  username: string
  onUsername: (value: string) => void
  token: string
  onToken: (value: string) => void
  hint?: string
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
      <FormField label={t("add.token")} hint={hint} errors={errors}>
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
export function AddDeployKey({
  applying,
  reveal,
  host,
  page,
}: {
  applying: boolean
  reveal: DeployKeyReveal
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
