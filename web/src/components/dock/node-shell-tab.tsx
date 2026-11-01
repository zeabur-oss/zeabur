import { useQuery } from "@tanstack/react-query"
import { useEffect, useState } from "react"

import { nodeShellsQuery } from "@/api/node-shells"
import { Session } from "@/components/dock/session"
import { LoadingRows } from "@/components/page"
import { NodeShellDialog } from "@/components/node-shell"
import { Problem } from "@/components/problem"
import { useT } from "@/i18n"
import type { DockTab } from "@/lib/dock"
import { NODE_SHELL_NAMESPACE, nodeShellUrl } from "@/model/node-shell"
import { Button } from "@zeabur-oss/design-system/components/button"

type NodeShellTabProps = {
  tab: Extract<DockTab, { kind: "node" }>
  shown: boolean
  focus: number
}

/** A root shell on the node, once its pod runs. Ending the session removes the pod. */
export function NodeShellTab({ tab, shown, focus }: NodeShellTabProps) {
  const t = useT()
  const list = useQuery(nodeShellsQuery)
  const shell = list.data?.shells.find((s) => s.name === tab.pod)
  const [attempt, setAttempt] = useState(0)
  // Once connected the screen stays, whatever becomes of the pod.
  const [entered, setEntered] = useState(false)
  const running = shell?.phase === "running"
  useEffect(() => {
    if (running) setEntered(true)
  }, [running])

  if (entered || running)
    return (
      <Session
        url={nodeShellUrl(window.location, tab.pod)}
        attempt={attempt}
        project={NODE_SHELL_NAMESPACE}
        shown={shown}
        focus={focus}
        grant={false}
        toolbar={
          <span className="truncate font-mono" translate="no">
            {`root@${tab.node}`}
          </span>
        }
        ended={<Ended pod={tab.pod} />}
        onReconnect={() => setAttempt(attempt + 1)}
      />
    )
  if (list.isPending) return <LoadingRows label={t("common.loading")} rows={2} />
  if (list.isError) return <Problem error={list.error} />
  if (!shell || shell.phase === "ended") return <Ended pod={tab.pod} />
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto px-3 py-3 text-sm">
      <output className="text-secondary">{t("nodeShell.starting", tab.node)}</output>
      {shell.reason && shell.reason !== "ContainerCreating" ? (
        <p className="text-warn-strong">
          <span translate="no">{shell.reason}</span>
          {shell.message ? (
            <>
              {t("common.sep")}
              <span translate="no">{shell.message}</span>
            </>
          ) : null}
        </p>
      ) : null}
    </div>
  )
}

/** The server deletes the pod when the session ends; one left behind is removed here. */
function Ended({ pod }: { pod: string }) {
  const t = useT()
  const list = useQuery({ ...nodeShellsQuery, refetchOnMount: "always" })
  const [removing, setRemoving] = useState(false)
  const left = list.data?.shells.some((s) => s.name === pod)
  return (
    <div
      role="status"
      className="mx-3 mb-2 flex shrink-0 flex-wrap items-center gap-x-3 gap-y-2 rounded-lg border border-main bg-muted px-3 py-2"
    >
      <p className="min-w-0 flex-1 text-sm">
        {list.isFetching ? t("nodeShell.ended") : left ? t("nodeShell.left") : t("nodeShell.gone")}
      </p>
      {left && !list.isFetching ? (
        <Button size="sm" onClick={() => setRemoving(true)}>
          {t("nodeShell.remove")}
        </Button>
      ) : null}
      {removing ? (
        <NodeShellDialog request={{ op: "remove", pod }} onClose={() => setRemoving(false)} />
      ) : null}
    </div>
  )
}
