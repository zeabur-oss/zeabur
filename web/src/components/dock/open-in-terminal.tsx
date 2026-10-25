import { useQuery } from "@tanstack/react-query"
import TerminalWindowIcon from "~icons/ph/terminal-window"

import { toProblem } from "@/api/client"
import { clusterToolsQuery } from "@/api/cluster-tools"
import { useMayActAsClusterAdmin } from "@/components/gate"
import { useT } from "@/i18n"
import { openClusterKubectl } from "@/lib/dock"
import { isRefusal } from "@/model/problem"
import { Button } from "@zeabur/design-system/components/button"

/** Opens the cluster's kubectl terminal. Absent on the person's computer, and to anyone who is not
 * a cluster administrator and may not take admin rights. */
export function OpenClusterKubectl() {
  const t = useT()
  const admin = useMayActAsClusterAdmin()
  const tools = useQuery({ ...clusterToolsQuery, enabled: admin })
  // A refusal still offers it: the tab offers admin rights.
  const available = tools.data?.available ?? (tools.isError && isRefusal(toProblem(tools.error)))
  if (!available) return null
  return (
    <Button variant="outline" size="sm" onClick={() => openClusterKubectl()}>
      <TerminalWindowIcon aria-hidden="true" data-icon="inline-start" />
      {t("action.openToolsTerminal")}
    </Button>
  )
}
