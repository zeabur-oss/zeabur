// A plan for the cluster's GitHub App: storing it, or granting an installation to projects.
import { useMutation, useQuery } from "@tanstack/react-query"
import WarningIcon from "~icons/ph/warning"

import type { Schemas } from "@/api/client"
import { applyGitHubApp, refreshGitHubApp, type GitHubAppPlan } from "@/api/github-app"
import { PlanDiffs, PlanFrame } from "@/components/cluster-plan"
import { useT } from "@/i18n"
import { Alert, AlertDescription } from "@zeabur-oss/design-system/components/alert"

const warningText = {
  replacesApp: "githubApp.warning.replacesApp",
  noWebhook: "githubApp.warning.noWebhook",
  wasEveryNamespace: "githubApp.warning.wasEveryNamespace",
  removesNamespaces: "githubApp.warning.removesNamespaces",
} as const

function writeLine(write: Schemas.GitHubAppWrite, t: ReturnType<typeof useT>): string {
  if (write.kind === "Secret") return t("githubApp.plan.secret")
  if (write.name === "github-app-installations") return t("githubApp.plan.grants")
  return t("githubApp.plan.project", write.namespace)
}

function WriteList({ writes }: { writes: Schemas.GitHubAppWrite[] }) {
  const t = useT()
  if (writes.length === 0) return <p>{t("clusterBackups.plan.nothing")}</p>
  return (
    <ul className="flex flex-col gap-1">
      {writes.map((w) => (
        <li key={[w.namespace, w.kind, w.name].join("/")}>{writeLine(w, t)}</li>
      ))}
    </ul>
  )
}

export function GitHubAppPlanDialog({
  title,
  action,
  queryKey,
  plan: planOf,
  onDone,
  onClose,
}: {
  title: string
  action: string
  queryKey: unknown[]
  plan: (signal: AbortSignal) => Promise<GitHubAppPlan>
  onDone: () => void
  onClose: () => void
}) {
  const t = useT()
  const plan = useQuery({
    queryKey,
    queryFn: ({ signal }) => planOf(signal),
    retry: false,
    staleTime: Infinity,
    gcTime: 0,
    refetchOnWindowFocus: false,
  })
  const apply = useMutation({
    mutationFn: applyGitHubApp,
    onSuccess: () => {
      refreshGitHubApp()
      onDone()
    },
    onError: refreshGitHubApp,
  })
  return (
    <PlanFrame
      title={title}
      plan={plan}
      apply={apply}
      action={action}
      what={t("githubApp.what")}
      ready={(data) => (data?.writes.length ?? 0) > 0}
      onClose={onClose}
    >
      {(data) => (
        <>
          <WriteList writes={data.writes} />
          {data.warnings.length > 0 ? (
            <Alert variant="warn">
              <WarningIcon aria-hidden="true" />
              <AlertDescription>
                {t(
                  "common.sentences",
                  data.warnings.map((w) => t(warningText[w])),
                )}
              </AlertDescription>
            </Alert>
          ) : null}
          {data.writes.some((w) => w.kind === "Secret") ? (
            <p className="type-meta text-muted-foreground">{t("githubApp.plan.keysHidden")}</p>
          ) : null}
          <PlanDiffs writes={data.writes} />
        </>
      )}
    </PlanFrame>
  )
}
