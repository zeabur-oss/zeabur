// A node's menu: stop scheduling, resume scheduling. Each opens a plan the person reviews.
import { useMutation, useQuery } from "@tanstack/react-query"
import type { ComponentType } from "react"
import DotsThreeIcon from "~icons/ph/dots-three"
import PauseCircleIcon from "~icons/ph/pause-circle"
import PlayCircleIcon from "~icons/ph/play-circle"

import type { Schemas } from "@/api/client"
import { applyNodeMaintenance, nodesQuery, planNodeMaintenance, queryClient } from "@/api/queries"
import { useClusterGate } from "@/components/gate"
import { KubeDetails } from "@/components/kube-details"
import { Frame } from "@/components/members/frame"
import { LoadingRows } from "@/components/page"
import { useSecondsLeft } from "@/components/plan/expiry"
import { SealedFooter } from "@/components/plan/sealed-footer"
import { Problem } from "@/components/problem"
import { useT } from "@/i18n"
import { kubectlCommand } from "@/model/node-maintenance"
import { Button } from "@zeabur/design-system/components/button"
import { Menu, MenuContent, MenuItem, MenuTrigger } from "@zeabur/design-system/components/menu"

type NodeView = Schemas["NodeView"]
type NodeAction = Schemas["NodeAction"]

const refresh = () => {
  void queryClient.invalidateQueries({ queryKey: nodesQuery.queryKey })
}

interface Item {
  icon: ComponentType<{ "aria-hidden"?: boolean | "true" }>
  label: string
  action: NodeAction
}

/** The action on a node, for those who may change nodes; nothing for anyone else. */
export function NodeMenu({
  node,
  className,
  onOpen,
}: {
  node: NodeView
  className?: string
  onOpen: (action: NodeAction) => void
}) {
  const t = useT()
  const gate = useClusterGate("changeNodes")
  if (!gate.open) return null
  const {
    icon: Icon,
    label,
    action,
  }: Item = node.cordoned
    ? { icon: PlayCircleIcon, label: t("maint.uncordon"), action: "uncordon" }
    : { icon: PauseCircleIcon, label: t("maint.cordon"), action: "cordon" }
  return (
    <div className={className}>
      <Menu>
        <MenuTrigger
          render={
            <Button variant="ghost" size="icon-sm" aria-label={t("maint.actions", node.name)} />
          }
        >
          <DotsThreeIcon aria-hidden="true" />
        </MenuTrigger>
        <MenuContent align="end">
          <MenuItem onClick={() => onOpen(action)}>
            <Icon aria-hidden="true" />
            {label}
          </MenuItem>
        </MenuContent>
      </Menu>
    </div>
  )
}

/** The dialog open on a node. */
export function NodeDialog({
  node,
  action,
  onClose,
}: {
  node: string
  action?: NodeAction
  onClose: () => void
}) {
  if (!action) return null
  return <MaintenanceDialog key={action} node={node} action={action} onClose={onClose} />
}

function MaintenanceDialog({
  node,
  action,
  onClose,
}: {
  node: string
  action: NodeAction
  onClose: () => void
}) {
  const t = useT()
  const plan = useQuery({
    queryKey: ["node-maintenance-plan", node, action],
    queryFn: ({ signal }) => planNodeMaintenance(node, action, signal),
    retry: false,
    staleTime: Infinity,
    gcTime: 0,
  })
  const apply = useMutation({
    mutationFn: (token: string) => applyNodeMaintenance(node, { token }),
    onSuccess: () => {
      refresh()
      onClose()
    },
    onError: refresh,
  })
  const left = useSecondsLeft(plan.data?.expiresAt)
  const ready = plan.data && !plan.isFetching
  return (
    <Frame
      title={t(`maint.title.${action}`, node)}
      onClose={onClose}
      footer={
        <SealedFooter
          left={left}
          ready={Boolean(ready)}
          back={
            <Button variant="ghost" size="sm" onClick={onClose}>
              {t("plan.cancel")}
            </Button>
          }
          onReplan={() => void plan.refetch()}
        >
          <Button
            size="sm"
            disabled={!ready}
            loading={apply.isPending}
            onClick={() => plan.data && apply.mutate(plan.data.token)}
          >
            {t("plan.apply")}
          </Button>
        </SealedFooter>
      }
    >
      {plan.isPending || plan.isFetching ? (
        <LoadingRows label={t("plan.planning")} rows={3} />
      ) : plan.isError ? (
        <Problem error={plan.error} />
      ) : (
        <>
          <p>{t(`maint.lead.${action}`, plan.data.node)}</p>
          <KubeDetails
            facts={[[t("maint.kube.command"), kubectlCommand(action, plan.data.node)]]}
          />
        </>
      )}
      {apply.isError ? <Problem error={apply.error} /> : null}
    </Frame>
  )
}
