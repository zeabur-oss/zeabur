// Cluster → GitHub: the App Zeabur Builder builds private GitHub repositories with, made through
// GitHub's manifest flow, and which projects may use each installation.
import { useMutation, useQuery } from "@tanstack/react-query"
import { getRouteApi, useNavigate } from "@tanstack/react-router"
import { useId, useState } from "react"
import ArrowSquareOutIcon from "~icons/ph/arrow-square-out"
import GithubLogoIcon from "~icons/ph/github-logo"
import WarningIcon from "~icons/ph/warning"

import type { Schemas } from "@/api/client"
import { convertCode, gitHubAppQuery, planGrant, startManifest } from "@/api/github-app"
import { clusterQuery, projectsQuery } from "@/api/queries"
import { FormField } from "@/components/form-field"
import { GateTip, useHintGate } from "@/components/gate"
import { GitHubAppPlanDialog } from "@/components/github-app-plan"
import { Problem } from "@/components/problem"
import { permission, Refused } from "@/components/refused"
import { useT } from "@/i18n"
import { openAdd } from "@/lib/adding"
import {
  CALLBACK_PATH,
  appName,
  grantChoices,
  nameProblem,
  webhookProblem,
} from "@/model/github-app"
import type { Gate } from "@/model/rights"
import { ClusterFrame } from "@/routes/cluster/frame"
import { FormButtons, Section } from "@/routes/cluster/section"
import { Alert, AlertDescription } from "@zeabur/design-system/components/alert"
import { Button, buttonVariants } from "@zeabur/design-system/components/button"
import { Checkbox } from "@zeabur/design-system/components/checkbox"
import { Input } from "@zeabur/design-system/components/input"
import { Label } from "@zeabur/design-system/components/label"

type Installation = Schemas.GitHubInstallation

const formBox = "flex flex-col gap-4 rounded-xl border border-main bg-card p-5"

function External({ href, children }: { href: string; children: string }) {
  return (
    <a
      href={href}
      target="_blank"
      rel="noreferrer"
      className={buttonVariants({ size: "sm", variant: "outline" })}
    >
      {children}
      <ArrowSquareOutIcon aria-hidden="true" data-icon="inline-end" />
    </a>
  )
}

function GitHubAppSection({ project }: { project?: string }) {
  const t = useT()
  // Back from installing on GitHub in another tab, the installation shows at once.
  const data = useQuery({ ...gitHubAppQuery, refetchOnWindowFocus: true }).data
  const gate = useHintGate("changeGitHubApp", data?.canChange)
  if (!data) return null
  return (
    <Section
      id="github-app"
      title={t("githubApp.title")}
      action={
        data.app?.installUrl ? (
          <External href={data.app.installUrl}>{t("githubApp.install")}</External>
        ) : null
      }
    >
      <p className="type-meta text-muted-foreground">{t("githubApp.body")}</p>
      {data.app ? (
        <AppDetails data={data} gate={gate} />
      ) : (
        <NoApp data={data} gate={gate} project={project} />
      )}
    </Section>
  )
}

function NoApp({ data, gate, project }: { data: Schemas.GitHubApp; gate: Gate; project?: string }) {
  const t = useT()
  const [creating, setCreating] = useState(gate.open && project !== undefined)
  if (data.unreadable.some((u) => u.resource === "secrets"))
    return (
      <Refused
        refusal="unreadable"
        what={t("githubApp.what")}
        missing={[`${permission("get", "secrets")} -n ${data.namespace}`]}
        line
      />
    )
  if (creating)
    return <CreateForm gate={gate} project={project} onCancel={() => setCreating(false)} />
  return (
    <div className="flex flex-wrap items-center justify-between gap-3">
      <p className="type-body">{t("githubApp.none")}</p>
      <GateTip gate={gate}>
        <Button size="sm" variant="outline" onClick={() => setCreating(true)}>
          <GithubLogoIcon aria-hidden="true" data-icon="inline-start" />
          {t("githubApp.create")}
        </Button>
      </GateTip>
    </div>
  )
}

function CreateForm({
  gate,
  project,
  onCancel,
}: {
  gate: Gate
  project?: string
  onCancel: () => void
}) {
  const t = useT()
  const cluster = useQuery(clusterQuery).data
  const [name, setName] = useState(() => appName(cluster?.name ?? "cluster"))
  const [organization, setOrganization] = useState("")
  const [webhook, setWebhook] = useState("")
  const [tried, setTried] = useState(false)
  const start = useMutation({
    mutationFn: () =>
      startManifest({
        name: name.trim(),
        organization: organization.trim(),
        webhookUrl: webhook.trim(),
        redirectUrl: `${window.location.origin}${CALLBACK_PATH}`,
      }),
    onSuccess: (formUrl) => {
      rememberProject(project)
      window.location.assign(formUrl)
    },
  })
  const naming = nameProblem(name)
  const hook = webhookProblem(webhook)
  return (
    <form
      className={formBox}
      onSubmit={(event) => {
        event.preventDefault()
        setTried(true)
        if (!naming && !hook) start.mutate()
      }}
    >
      <FormField
        label={t("githubApp.field.name")}
        hint={t("githubApp.field.nameHint")}
        errors={tried && naming ? [t(`githubApp.problem.name.${naming}`)] : []}
      >
        <Input size="sm" value={name} onChange={(event) => setName(event.target.value)} />
      </FormField>
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        <FormField
          label={t("githubApp.field.organization")}
          hint={t("githubApp.field.organizationHint")}
        >
          <Input
            size="sm"
            mono
            value={organization}
            autoComplete="off"
            spellCheck={false}
            onChange={(event) => setOrganization(event.target.value)}
          />
        </FormField>
        <FormField
          label={t("githubApp.field.webhook")}
          hint={t("githubApp.field.webhookHint")}
          errors={tried && hook ? [t("githubApp.problem.webhook")] : []}
        >
          <Input
            size="sm"
            mono
            value={webhook}
            placeholder="https://builder.example.com/hooks/github-app"
            autoComplete="off"
            spellCheck={false}
            onChange={(event) => setWebhook(event.target.value)}
          />
        </FormField>
      </div>
      {start.isError ? (
        <Alert variant="warn">
          <WarningIcon aria-hidden="true" />
          <AlertDescription>{start.error.message}</AlertDescription>
        </Alert>
      ) : null}
      <div className="flex justify-end gap-2">
        <Button size="sm" variant="ghost" type="button" onClick={onCancel}>
          {t("clusterBackups.cancel")}
        </Button>
        <GateTip gate={gate}>
          <Button size="sm" type="submit" loading={start.isPending}>
            {t("githubApp.continue")}
          </Button>
        </GateTip>
      </div>
    </form>
  )
}

function AppDetails({ data, gate }: { data: Schemas.GitHubApp; gate: Gate }) {
  const t = useT()
  const [granting, setGranting] = useState<number>()
  const [grant, setGrant] = useState<{ installation: Installation; namespaces: string[] }>()
  const installations = data.installations ?? []
  return (
    <>
      <AppLink app={data.app} />
      <InstallationsNote data={data} />
      {installations.length > 0 ? (
        <ul
          aria-label={t("githubApp.installations")}
          className="flex flex-col divide-y divide-main rounded-lg border border-main bg-card"
        >
          {installations.map((i) => (
            <InstallationRow
              key={i.id}
              installation={i}
              gate={gate}
              granting={granting === i.id}
              onGrant={(on) => setGranting(on ? i.id : undefined)}
              onReview={(namespaces) => {
                setGranting(undefined)
                setGrant({ installation: i, namespaces })
              }}
            />
          ))}
        </ul>
      ) : null}
      {grant ? (
        <GitHubAppPlanDialog
          title={t("githubApp.plan.grant", grant.installation.account)}
          action={t("plan.apply")}
          queryKey={["github-app-grant", grant.installation.id, grant.namespaces]}
          plan={(signal) =>
            planGrant(
              { installationId: grant.installation.id, namespaces: grant.namespaces },
              signal,
            )
          }
          onDone={() => setGrant(undefined)}
          onClose={() => setGrant(undefined)}
        />
      ) : null}
    </>
  )
}

function AppLink({ app }: { app: Schemas.GitHubApp["app"] }) {
  const t = useT()
  return (
    <div className="flex min-w-0 flex-wrap items-center gap-x-4 gap-y-2">
      <a
        href={app?.htmlUrl}
        target="_blank"
        rel="noreferrer"
        className="inline-flex min-w-0 items-center gap-1.5 type-body font-medium text-foreground hover:underline"
      >
        <GithubLogoIcon aria-hidden="true" className="size-4 shrink-0" />
        <span className="truncate">{t("githubApp.open")}</span>
        <ArrowSquareOutIcon aria-hidden="true" className="size-3.5 shrink-0" />
      </a>
      {app?.webhook ? null : (
        <span className="type-meta text-muted-foreground">{t("githubApp.pollsOnly")}</span>
      )}
    </div>
  )
}

/** Why no installation shows: GitHub did not answer, or the App is installed nowhere yet. */
function InstallationsNote({ data }: { data: Schemas.GitHubApp }) {
  const t = useT()
  if (data.installationsProblem) return <Problem problem={data.installationsProblem} />
  if (data.installations?.length === 0)
    return <p className="type-body">{t("githubApp.noInstallations")}</p>
  return null
}

function grantedTo(t: ReturnType<typeof useT>, installation: Installation): string {
  if (installation.everyNamespace) return t("githubApp.everyProject")
  if (installation.namespaces.length === 0) return t("githubApp.noProject")
  return t("common.list", installation.namespaces)
}

function InstallationRow({
  installation: i,
  gate,
  granting,
  onGrant,
  onReview,
}: {
  installation: Installation
  gate: Gate
  granting: boolean
  onGrant: (on: boolean) => void
  onReview: (namespaces: string[]) => void
}) {
  const t = useT()
  return (
    <li className="flex min-w-0 flex-col gap-3 px-4 py-2.5">
      <div className="flex min-w-0 flex-wrap items-center gap-x-4 gap-y-1">
        <div className="flex min-w-0 flex-1 flex-col">
          <span className="truncate type-body font-medium" translate="no">
            {i.account}
          </span>
          <span className="truncate type-meta text-muted-foreground">
            {t(`githubApp.selection.${i.repositorySelection === "all" ? "all" : "selected"}`)}
            {t("common.sep")}
            {grantedTo(t, i)}
          </span>
        </div>
        {granting ? null : (
          <GateTip gate={gate}>
            <Button size="xs" variant="ghost" onClick={() => onGrant(true)}>
              {t("githubApp.choose")}
            </Button>
          </GateTip>
        )}
      </div>
      {granting ? (
        <GrantForm
          installation={i}
          gate={gate}
          onCancel={() => onGrant(false)}
          onReview={onReview}
        />
      ) : null}
    </li>
  )
}

function GrantForm({
  installation,
  gate,
  onCancel,
  onReview,
}: {
  installation: Installation
  gate: Gate
  onCancel: () => void
  onReview: (namespaces: string[]) => void
}) {
  const t = useT()
  const legend = useId()
  const projects = useQuery(projectsQuery).data
  const choices = grantChoices(
    (projects?.items ?? []).filter((p) => p.adopted && !p.system).map((p) => p.name),
    installation.namespaces,
  )
  const [chosen, setChosen] = useState<string[]>(installation.namespaces)
  const toggle = (name: string, on: boolean) =>
    setChosen((c) => (on ? [...c, name] : c.filter((n) => n !== name)))
  return (
    <form
      className="flex flex-col gap-3"
      onSubmit={(event) => {
        event.preventDefault()
        onReview([...chosen].sort())
      }}
    >
      <p id={legend} className="type-meta text-muted-foreground">
        {t("githubApp.grantHint")}
      </p>
      <ul
        aria-labelledby={legend}
        className="flex max-h-56 flex-col divide-y divide-main overflow-y-auto rounded-lg border border-main"
      >
        {choices.map((name) => (
          <li key={name} className="flex h-10 min-w-0 items-center px-3">
            <Label className="min-w-0">
              <Checkbox
                checked={chosen.includes(name)}
                onCheckedChange={(on) => toggle(name, on)}
              />
              <span className="min-w-0 truncate type-mono" translate="no">
                {name}
              </span>
            </Label>
          </li>
        ))}
      </ul>
      <FormButtons gate={gate} onCancel={onCancel} />
    </form>
  )
}

const callbackRoute = getRouteApi("/cluster/github-app")

// The project whose Git form led here, kept through GitHub's manifest flow, which takes the
// browser to GitHub and back.
const RETURN_KEY = "zeabur.github-app.project"

function rememberProject(project: string | undefined) {
  try {
    if (project) window.sessionStorage.setItem(RETURN_KEY, project)
    else window.sessionStorage.removeItem(RETURN_KEY)
  } catch {
    // Storage unavailable: the page offers no way back to the project.
  }
}

function rememberedProject(): string | undefined {
  try {
    return window.sessionStorage.getItem(RETURN_KEY) ?? undefined
  } catch {
    return undefined
  }
}

/** Cluster → GitHub: the App, and where GitHub sends the browser back after making it. */
export function GitHubAppPage() {
  const { code, state, ...search } = callbackRoute.useSearch()
  const project = search.project ?? (code ? rememberedProject() : undefined)
  return (
    <ClusterFrame tab="github">
      {project ? <BackToProject project={project} /> : null}
      <Callback code={code} state={state} project={project} />
      <GitHubAppSection project={project} />
    </ClusterFrame>
  )
}

function Callback({ code, state, project }: { code?: string; state?: string; project?: string }) {
  const t = useT()
  const navigate = useNavigate()
  if (!code && !state) return null
  if (!code || !state)
    return (
      <Alert variant="warn">
        <WarningIcon aria-hidden="true" />
        <AlertDescription>{t("githubApp.callback.missing")}</AlertDescription>
      </Alert>
    )
  const back = () => void navigate({ to: "/cluster/github-app", search: { project } })
  return (
    <GitHubAppPlanDialog
      title={t("githubApp.plan.save")}
      action={t("githubApp.plan.save")}
      queryKey={["github-app-conversion", code]}
      plan={(signal) => convertCode({ code, state }, signal)}
      onDone={back}
      onClose={back}
    />
  )
}

function BackToProject({ project }: { project: string }) {
  const t = useT()
  const navigate = useNavigate()
  return (
    <div className="flex min-w-0 flex-wrap items-center gap-x-4 gap-y-2 rounded-lg border border-main bg-card px-3 py-2.5">
      <p className="min-w-0 flex-1 type-meta text-muted-foreground">
        {t("githubApp.return.body", project)}
      </p>
      <Button
        size="sm"
        variant="outline"
        onClick={() => {
          rememberProject(undefined)
          void navigate({ to: "/projects/$project", params: { project } })
          openAdd(project, "git")
        }}
      >
        {t("githubApp.return.action")}
      </Button>
    </div>
  )
}
