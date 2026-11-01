// "Deploy from Git" before the project can pick GitHub repositories: the step that gets it there,
// whether that is making the cluster's App, installing it on GitHub or granting it to the project.
import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { useState } from "react"
import ArrowSquareOutIcon from "~icons/ph/arrow-square-out"
import CaretDownIcon from "~icons/ph/caret-down"
import GithubLogoIcon from "~icons/ph/github-logo"

import type { Schemas } from "@/api/client"
import { gitHubAppQuery, planGrant } from "@/api/github-app"
import { GitHubAppPlanDialog } from "@/components/github-app-plan"
import { useT } from "@/i18n"
import { adding } from "@/lib/adding"
import { connectStep, withProject, type ConnectStep } from "@/model/github-app"
import { Button, buttonVariants } from "@zeabur-oss/design-system/components/button"
import { Menu, MenuContent, MenuItem, MenuTrigger } from "@zeabur-oss/design-system/components/menu"

type Installation = Schemas.GitHubInstallation

export function GitHubConnect({ project }: { project: string }) {
  const t = useT()
  // Back from installing on GitHub in another tab, the installation shows at once.
  const data = useQuery({ ...gitHubAppQuery, refetchOnWindowFocus: true }).data
  const step = connectStep(data, project)
  const [granting, setGranting] = useState<Installation>()
  if (!step) return null
  return (
    <div
      data-testid="github-connect"
      className="flex min-w-0 flex-wrap items-center gap-x-4 gap-y-2 rounded-lg border border-main bg-card px-3 py-2.5"
    >
      <GithubLogoIcon aria-hidden="true" className="size-5 shrink-0" />
      <p className="min-w-0 flex-1 type-meta text-muted-foreground">{lineOf(t, step)}</p>
      <Action project={project} step={step} onGrant={setGranting} />
      {granting ? (
        <GitHubAppPlanDialog
          title={t("add.githubGrant", granting.account, project)}
          action={t("plan.apply")}
          queryKey={["github-app-grant", granting.id, project]}
          plan={(signal) =>
            planGrant(
              {
                installationId: granting.id,
                namespaces: withProject(granting.namespaces, project),
              },
              signal,
            )
          }
          onDone={() => setGranting(undefined)}
          onClose={() => setGranting(undefined)}
        />
      ) : null}
    </div>
  )
}

function lineOf(t: ReturnType<typeof useT>, step: ConnectStep): string {
  switch (step.step) {
    case "ask":
      return t("add.githubConnect.ask")
    case "problem":
      return t("add.githubConnect.problem")
    case "grant":
      return t(
        "add.githubConnect.grant",
        t(
          "common.list",
          step.installations.map((i) => i.account),
        ),
      )
    case "create":
    case "install":
      return t("add.githubConnect.body")
  }
}

function Action({
  project,
  step,
  onGrant,
}: {
  project: string
  step: ConnectStep
  onGrant: (installation: Installation) => void
}) {
  const t = useT()
  const outline = buttonVariants({ size: "sm", variant: "outline" })
  switch (step.step) {
    case "ask":
      return null
    case "install":
      if (step.installUrl)
        return (
          <a href={step.installUrl} target="_blank" rel="noreferrer" className={outline}>
            {t("add.githubConnect.action")}
            <ArrowSquareOutIcon aria-hidden="true" data-icon="inline-end" />
          </a>
        )
      return <ClusterLink project={project} label={t("add.githubConnect.action")} />
    case "create":
      return <ClusterLink project={project} label={t("add.githubConnect.action")} />
    case "problem":
      return <ClusterLink project={project} label={t("add.githubConnect.look")} />
    case "grant":
      return <GrantButton installations={step.installations} onGrant={onGrant} />
  }
}

function ClusterLink({ project, label }: { project: string; label: string }) {
  return (
    <Link
      to="/cluster/github-app"
      search={{ project }}
      onClick={() => adding.set(null)}
      className={buttonVariants({ size: "sm", variant: "outline" })}
    >
      {label}
    </Link>
  )
}

function GrantButton({
  installations,
  onGrant,
}: {
  installations: Installation[]
  onGrant: (installation: Installation) => void
}) {
  const t = useT()
  const [only] = installations
  if (only && installations.length === 1)
    return (
      <Button type="button" size="sm" variant="outline" onClick={() => onGrant(only)}>
        {t("add.githubConnect.action")}
      </Button>
    )
  return (
    <Menu>
      <MenuTrigger render={<Button type="button" size="sm" variant="outline" />}>
        {t("add.githubConnect.action")}
        <CaretDownIcon aria-hidden="true" data-icon="inline-end" />
      </MenuTrigger>
      <MenuContent align="end">
        {installations.map((i) => (
          <MenuItem key={i.id} onClick={() => onGrant(i)}>
            <span translate="no">{i.account}</span>
          </MenuItem>
        ))}
      </MenuContent>
    </Menu>
  )
}
