// The plan that starts the person's kubectl terminal for the whole cluster, for cluster
// administrators.
import { useMutation, useQuery } from "@tanstack/react-query"

import {
  applyClusterTools,
  planClusterTools,
  refreshClusterTools,
  type ClusterToolsPlan,
} from "@/api/cluster-tools"
import { PlanFooter, PlanWrites } from "@/components/cluster-plan"
import { Consequence, Frame } from "@/components/members/frame"
import { LoadingRows } from "@/components/page"
import { terminalLines } from "@/components/plan/describe"
import { Problem } from "@/components/problem"
import { useT } from "@/i18n"

export function ClusterToolsDialog({
  onClose,
  onApplied,
}: {
  onClose: () => void
  onApplied: (pod: string) => void
}) {
  const t = useT()
  // Planning writes nothing, so it is a query.
  const plan = useQuery({
    queryKey: ["cluster-tools-plan"],
    queryFn: ({ signal }) => planClusterTools(signal),
    retry: false,
    staleTime: Infinity,
    gcTime: 0,
  })
  const apply = useMutation({
    mutationFn: (token: string) => applyClusterTools(token),
    onSuccess: (applied) => {
      refreshClusterTools()
      onClose()
      onApplied(applied.pod)
    },
    onError: refreshClusterTools,
  })
  return (
    <Frame
      title={t("clusterTools.title")}
      wide
      onClose={onClose}
      footer={
        <PlanFooter
          expiresAt={plan.data?.expiresAt}
          ready={!!plan.data && !plan.isFetching}
          token={plan.data?.token}
          action={t("action.openToolsTerminal")}
          applying={apply.isPending}
          onCancel={onClose}
          onReplan={() => void plan.refetch()}
          onApply={(token) => apply.mutate(token)}
        />
      }
    >
      {plan.isPending || plan.isFetching ? (
        <LoadingRows label={t("plan.planning")} rows={3} />
      ) : plan.isError ? (
        <Problem
          error={plan.error}
          what={t("clusterTools.what")}
          retry={() => void plan.refetch()}
        />
      ) : (
        <Planned plan={plan.data} />
      )}
      {apply.isError ? <Problem error={apply.error} what={t("clusterTools.what")} /> : null}
    </Frame>
  )
}

function Planned({ plan }: { plan: ClusterToolsPlan }) {
  const t = useT()
  const exposed = plan.warnings.find((w) => w.code === "toolsCredentialExposed")
  return (
    <>
      {exposed ? (
        <Consequence>{t("warn.toolsCredentialExposed", plan.namespace)}</Consequence>
      ) : null}
      {plan.writes.length > 0 ? <p>{t("clusterTools.starts", plan.namespace)}</p> : null}
      <ul className="flex list-disc flex-col gap-1 pl-5">
        {terminalLines(t, plan).map((line) => (
          <li key={line}>{line}</li>
        ))}
      </ul>
      {plan.writes.length > 0 ? <PlanWrites writes={plan.writes} /> : null}
    </>
  )
}
