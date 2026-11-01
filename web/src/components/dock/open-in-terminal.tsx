import { useQuery } from "@tanstack/react-query"
import TerminalWindowIcon from "~icons/ph/terminal-window"

import { clusterToolsQuery } from "@/api/cluster-tools"
import { useClusterAdmin } from "@/components/gate"
import { useT } from "@/i18n"
import { openClusterKubectl } from "@/lib/dock"
import { Button } from "@zeabur-oss/design-system/components/button"

/** Opens the cluster's kubectl terminal. Absent on the person's computer, and to anyone who is not
 * a cluster administrator. */
export function OpenClusterKubectl() {
  const t = useT()
  const admin = useClusterAdmin()
  const tools = useQuery({ ...clusterToolsQuery, enabled: admin })
  if (!tools.data?.available) return null
  return (
    <Button variant="outline" size="sm" onClick={() => openClusterKubectl()}>
      <TerminalWindowIcon aria-hidden="true" data-icon="inline-start" />
      {t("action.openToolsTerminal")}
    </Button>
  )
}
