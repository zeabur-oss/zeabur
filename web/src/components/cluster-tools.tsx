// The plan that starts the person's kubectl terminal for the whole cluster, for cluster
// administrators.
import { useMutation, useQuery } from "@tanstack/react-query"

import {
  applyClusterTools,
  planClusterTools,
  refreshClusterTools,
  type ClusterToolsPlan,
} from "@/api/cluster-tools"
import { Exposed, PlanFrame, PlanWrites } from "@/components/cluster-plan"
import { Consequence } from "@/components/members/frame"
import { terminalLines } from "@/components/plan/describe"
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
    <PlanFrame
      title={t("clusterTools.title")}
      plan={plan}
      apply={apply}
      action={t("action.openToolsTerminal")}
      what={t("clusterTools.what")}
      onClose={onClose}
    >
      {(data) => <Planned plan={data} />}
    </PlanFrame>
  )
}

function Planned({ plan }: { plan: ClusterToolsPlan }) {
  const t = useT()
  const exposed = plan.warnings.find((w) => w.code === "toolsCredentialExposed")
  return (
    <>
      {plan.exposed.length > 0 ? (
        <Exposed namespace={plan.namespace} exposed={plan.exposed} />
      ) : null}
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
