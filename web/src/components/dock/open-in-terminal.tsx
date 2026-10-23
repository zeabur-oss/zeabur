import { useQuery } from "@tanstack/react-query"
import TerminalWindowIcon from "~icons/ph/terminal-window"

import { toProblem } from "@/api/client"
import { clusterToolsQuery } from "@/api/cluster-tools"
import { toolsQuery } from "@/api/queries"
import { useMayActAsClusterAdmin } from "@/components/gate"
import { useT } from "@/i18n"
import { openClusterKubectl, openKubectl } from "@/lib/dock"
import { isRefusal } from "@/model/problem"
import { Button } from "@zeabur/design-system/components/button"

/** Types the command into the project's kubectl terminal, or the cluster's without a project, for
 * the person to run. Absent where there is none: on the person's computer, when their tools pods
 * cannot be listed, or for the cluster's, to anyone who is not a cluster administrator and may not
 * take admin rights. `onOpen` closes what covers the dock. */
export function OpenInTerminal({
  project,
  command,
  label,
  variant = "ghost",
  className,
  onOpen,
}: {
  project?: string
  command: string
  label?: string
  variant?: "ghost" | "outline"
  className?: string
  onOpen?: () => void
}) {
  const t = useT()
  const admin = useMayActAsClusterAdmin()
  const tools = useQuery({
    ...(project === undefined ? clusterToolsQuery : toolsQuery(project)),
    enabled: project !== undefined || admin,
  })
  // A refusal still offers it: the tab offers admin rights.
  const available =
    tools.data?.available ??
    (project === undefined && tools.isError && isRefusal(toProblem(tools.error)))
  if (!available) return null
  return (
    <Button
      variant={variant}
      size="sm"
      className={className}
      onClick={() => {
        onOpen?.()
        if (project === undefined) openClusterKubectl({ command })
        else openKubectl(project, { command })
      }}
    >
      <TerminalWindowIcon aria-hidden="true" data-icon="inline-start" />
      {label ?? t("action.openInToolsTerminal")}
    </Button>
  )
}
