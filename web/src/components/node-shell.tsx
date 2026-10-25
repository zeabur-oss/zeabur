// A root shell on a node, for cluster administrators: the plan that opens or removes its pod, and
// the pods left on a node.
import { useMutation, useQuery } from "@tanstack/react-query"
import { useState } from "react"
import CaretRightIcon from "~icons/ph/caret-right"
import TerminalIcon from "~icons/ph/terminal"

import {
  applyNodeShell,
  nodeShellsQuery,
  planNodeShell,
  refreshNodeShells,
  type NodeShell,
  type NodeShellPlan,
  type NodeShellRequest,
} from "@/api/node-shells"
import { Ago, CopyButton } from "@/components/bits"
import { Exposed, PlanFrame, PlanWrites } from "@/components/cluster-plan"
import { useClusterAdmin } from "@/components/gate"
import { Consequence } from "@/components/members/frame"
import { StatusDot } from "@/components/status"
import { useT } from "@/i18n"
import { openNodeShell } from "@/lib/dock"
import { enters, shellHealth, shellsOn, shellWord } from "@/model/node-shell"
import { Button } from "@zeabur/design-system/components/button"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur/design-system/components/collapsible"
import { Tooltip } from "@zeabur/design-system/components/tooltip"

/** A node row's root shell: the action, then the shell pods on the node. */
export function NodeShells({ node }: { node: string }) {
  const t = useT()
  const allowed = useClusterAdmin()
  const [opening, setOpening] = useState(false)
  if (!allowed) return null
  return (
    <>
      <Tooltip content={t("nodeShell.action")}>
        <Button
          variant="ghost"
          size="sm"
          className="-my-1 self-start max-sm:-ml-3 sm:order-last sm:self-center"
          aria-label={t("nodeShell.actionOn", node)}
          onClick={() => setOpening(true)}
        >
          <TerminalIcon aria-hidden="true" data-icon="inline-start" />
          <span className="sm:sr-only">{t("nodeShell.action")}</span>
        </Button>
      </Tooltip>
      <ShellPods node={node} />
      {opening ? (
        <NodeShellDialog
          request={{ op: "open", node }}
          onClose={() => setOpening(false)}
          onApplied={(pod) => openNodeShell(node, pod)}
        />
      ) : null}
    </>
  )
}

function ShellPods({ node }: { node: string }) {
  const t = useT()
  const list = useQuery(nodeShellsQuery)
  const [removing, setRemoving] = useState<string>()
  const shells = shellsOn(list.data?.shells ?? [], node)
  if (shells.length === 0) return null
  return (
    <ul aria-label={t("nodeShell.pods", node)} className="order-last flex w-full flex-col gap-1">
      {shells.map((shell) => (
        <li
          key={shell.name}
          className="flex flex-wrap items-center gap-x-3 gap-y-1 rounded-lg bg-plane px-3 py-1.5 type-meta"
        >
          <span className="flex items-center gap-2">
            <StatusDot health={shellHealth(shell)} />
            <ShellWord shell={shell} />
          </span>
          <span className="min-w-0 truncate type-mono text-muted-foreground" translate="no">
            {shell.name}
          </span>
          <span className="text-muted-foreground">
            {shell.mine ? t("nodeShell.byYou") : t("nodeShell.by", shell.person ?? "—")}
            {shell.createdAt ? (
              <>
                {t("common.sep")}
                <Ago iso={shell.createdAt} />
              </>
            ) : null}
          </span>
          <span className="ml-auto flex items-center gap-1">
            {enters(shell) ? (
              <Button size="xs" variant="outline" onClick={() => openNodeShell(node, shell.name)}>
                {t("tools.open")}
              </Button>
            ) : null}
            <Button size="xs" variant="ghost" onClick={() => setRemoving(shell.name)}>
              {t("nodeShell.remove")}
            </Button>
          </span>
        </li>
      ))}
      {removing ? (
        <NodeShellDialog
          request={{ op: "remove", pod: removing }}
          onClose={() => setRemoving(undefined)}
        />
      ) : null}
    </ul>
  )
}

function ShellWord({ shell }: { shell: NodeShell }) {
  const t = useT()
  const word = t(`nodeShell.word.${shellWord(shell)}`)
  if (shellWord(shell) !== "stuck" || !shell.message) return <span>{word}</span>
  return (
    <Tooltip content={<span translate="no">{shell.message}</span>}>
      <span className="text-warn-strong">
        {word}
        {t("common.sep")}
        <span translate="no">{shell.reason}</span>
      </span>
    </Tooltip>
  )
}

/** Opening a shell or removing its pod, as a plan to review. */
export function NodeShellDialog({
  request,
  onClose,
  onApplied,
}: {
  request: NodeShellRequest
  onClose: () => void
  onApplied?: (pod: string) => void
}) {
  const t = useT()
  // Planning writes nothing, so it is a query.
  const plan = useQuery({
    queryKey: ["node-shell-plan", request],
    queryFn: ({ signal }) => planNodeShell(request, signal),
    retry: false,
    staleTime: Infinity,
    gcTime: 0,
  })
  const apply = useMutation({
    mutationFn: (token: string) => applyNodeShell(token),
    onSuccess: (applied) => {
      refreshNodeShells()
      onClose()
      onApplied?.(applied.pod)
    },
    onError: refreshNodeShells,
  })
  const open = request.op === "open"
  return (
    <PlanFrame
      title={open ? t("nodeShell.openTitle", request.node) : t("nodeShell.removeTitle")}
      plan={plan}
      apply={apply}
      action={open ? t("nodeShell.open") : t("nodeShell.remove")}
      destructive={!open}
      what={t("nodeShell.what")}
      onClose={onClose}
    >
      {(data) =>
        open ? (
          <OpenPlan plan={data} />
        ) : (
          <>
            <p>
              {t("nodeShell.removeLine", data.pod, data.node)}
              {data.person ? ` ${t("nodeShell.openedBy", data.person)}` : ""}
            </p>
            <PlanWrites writes={data.writes} />
          </>
        )
      }
    </PlanFrame>
  )
}

function OpenPlan({ plan }: { plan: NodeShellPlan }) {
  const t = useT()
  const minutes = (seconds: number) => Math.round(seconds / 60)
  return (
    <>
      <Consequence>{t("nodeShell.root", plan.node)}</Consequence>
      <ul className="flex list-disc flex-col gap-1 pl-5">
        <li>{t("nodeShell.lasts", minutes(plan.deadlineSeconds))}</li>
        <li>{t("nodeShell.firstSession", minutes(plan.firstSessionSeconds))}</li>
        <li>{t("nodeShell.recorded")}</li>
      </ul>
      {plan.exposed.length > 0 ? (
        <Exposed namespace={plan.namespace} exposed={plan.exposed} />
      ) : null}
      <PlanWrites writes={plan.writes} />
      {plan.kubectl ? (
        <Collapsible>
          <CollapsibleTrigger variant="disclosure">
            <CaretRightIcon aria-hidden="true" />
            {t("nodeShell.kubectl")}
          </CollapsibleTrigger>
          <CollapsibleContent>
            <div className="relative mt-3 min-w-0 rounded-lg border bg-muted/30">
              <CopyButton value={plan.kubectl} className="absolute top-2 right-2" />
              <pre
                className="overflow-x-auto py-3 pr-12 pl-3 font-mono text-xs leading-5 whitespace-pre-wrap"
                translate="no"
              >
                {plan.kubectl}
              </pre>
            </div>
          </CollapsibleContent>
        </Collapsible>
      ) : null}
    </>
  )
}
