import { useQuery } from "@tanstack/react-query"
import TerminalWindowIcon from "~icons/ph/terminal-window"

import { toolsQuery } from "@/api/queries"
import { useT } from "@/i18n"
import { openKubectl } from "@/lib/dock"
import { Button } from "@zeabur/design-system/components/button"

/** Types the command into the project's kubectl terminal for the person to run. Absent where
 * there is none: on the person's computer, or when their tools pods cannot be listed. `onOpen`
 * closes what covers the dock. */
export function OpenInTerminal({
  project,
  command,
  variant = "ghost",
  className,
  onOpen,
}: {
  project: string
  command: string
  variant?: "ghost" | "outline"
  className?: string
  onOpen?: () => void
}) {
  const t = useT()
  const tools = useQuery(toolsQuery(project))
  if (!tools.data?.available) return null
  return (
    <Button
      variant={variant}
      size="sm"
      className={className}
      onClick={() => {
        onOpen?.()
        openKubectl(project, { command })
      }}
    >
      <TerminalWindowIcon aria-hidden="true" data-icon="inline-start" />
      {t("action.openInToolsTerminal")}
    </Button>
  )
}
