// A node's menu: stop scheduling, maintenance mode, resume scheduling, remove. Each opens a plan the
// person reviews; maintenance then moves replicas off in passes while the dialog stays open.
import { useMutation, useQuery } from "@tanstack/react-query"
import { useEffect, useId, useRef, useState, type ComponentType, type ReactNode } from "react"
import DotsThreeIcon from "~icons/ph/dots-three"
import InfoIcon from "~icons/ph/info-fill"
import PauseCircleIcon from "~icons/ph/pause-circle"
import PlayCircleIcon from "~icons/ph/play-circle"
import TrashIcon from "~icons/ph/trash"
import WarningIcon from "~icons/ph/warning-fill"
import WrenchIcon from "~icons/ph/wrench"

import type { Schemas } from "@/api/client"
import {
  applyNodeMaintenance,
  evictNodePods,
  nodePoliciesQuery,
  nodesQuery,
  planNodeMaintenance,
  queryClient,
} from "@/api/queries"
import { useClusterGate } from "@/components/gate"
import { KubeDetails } from "@/components/kube-details"
import { Frame } from "@/components/members/frame"
import { LoadingRows } from "@/components/page"
import { COUNTDOWN_FROM_SECONDS, clock, useSecondsLeft } from "@/components/plan/expiry"
import { Problem } from "@/components/problem"
import { StatusDot } from "@/components/status"
import { YamlBlock } from "@/components/yaml"
import { useT } from "@/i18n"
import { useQuantityFormat } from "@/lib/quantity"
import { cn } from "@/lib/utils"
import { cordonedKind } from "@/model/node-health"
import {
  byProject,
  drain,
  fates,
  kubectlCommand,
  sleep,
  type PodGroup,
  UNINSTALL_COMMANDS,
  zeaburLeaving,
} from "@/model/node-maintenance"
import type { Health } from "@/model/service"
import { Alert, AlertDescription } from "@zeabur/design-system/components/alert"
import { Button } from "@zeabur/design-system/components/button"
import { Checkbox } from "@zeabur/design-system/components/checkbox"
import { Label } from "@zeabur/design-system/components/label"
import { Menu, MenuContent, MenuItem, MenuTrigger } from "@zeabur/design-system/components/menu"

type NodeView = Schemas["NodeView"]
type NodeAction = Schemas["NodeAction"]
type DrainProgress = Schemas["DrainProgress"]
type MaintenancePlan = Schemas["MaintenancePlan"]

const refresh = () => {
  void queryClient.invalidateQueries({ queryKey: nodesQuery.queryKey })
}

interface Item {
  key: string
  icon: ComponentType<{ "aria-hidden"?: boolean | "true" }>
  label: string
  action: NodeAction
}

/** The actions on a node, for those who may change nodes; nothing for anyone else. The row goes
 * once the node is removed, so `onRemoved` shows what to do next outside it. */
export function NodeMenu({
  node,
  className,
  onRemoved,
}: {
  node: NodeView
  className?: string
  onRemoved: (node: string) => void
}) {
  const t = useT()
  const gate = useClusterGate("changeNodes")
  // `removing`: maintenance was opened on the way to removing the node.
  const [open, setOpen] = useState<{ action: NodeAction; removing?: boolean }>()
  if (!gate.open) return null
  const left = node.cordoned && cordonedKind(node) !== "maintenance"
  const items: Item[] = [
    ...(node.cordoned
      ? []
      : [
          {
            key: "cordon",
            icon: PauseCircleIcon,
            label: t("maint.cordon"),
            action: "cordon" as const,
          },
        ]),
    ...(!node.cordoned || left
      ? [
          {
            key: "drain",
            icon: WrenchIcon,
            label: node.cordoned ? t("maint.continue") : t("maint.drain"),
            action: "drain" as const,
          },
        ]
      : []),
    ...(node.cordoned
      ? [
          {
            key: "uncordon",
            icon: PlayCircleIcon,
            label: t("maint.uncordon"),
            action: "uncordon" as const,
          },
        ]
      : []),
    { key: "remove", icon: TrashIcon, label: t("maint.remove"), action: "remove" },
  ]
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
          {items.map(({ key, icon: Icon, label, action }) => (
            <MenuItem key={key} onClick={() => setOpen({ action })}>
              <Icon aria-hidden="true" />
              {label}
            </MenuItem>
          ))}
        </MenuContent>
      </Menu>
      {open ? (
        <MaintenanceDialog
          key={open.action}
          node={node.name}
          action={open.action}
          removing={open.removing}
          onAction={(action) =>
            setOpen({ action, removing: open.removing || open.action === "remove" })
          }
          onRemoved={() => {
            setOpen(undefined)
            onRemoved(node.name)
          }}
          onClose={() => setOpen(undefined)}
        />
      ) : null}
    </div>
  )
}

function MaintenanceDialog({
  node,
  action,
  removing,
  onAction,
  onRemoved,
  onClose,
}: {
  node: string
  action: NodeAction
  removing?: boolean
  onAction: (action: NodeAction) => void
  onRemoved: () => void
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
  const [includeStuck, setIncludeStuck] = useState(false)
  const [includeDeleted, setIncludeDeleted] = useState(false)
  const [loseData, setLoseData] = useState(false)
  const [progress, setProgress] = useState<DrainProgress>()
  const [running, setRunning] = useState(false)
  const [passError, setPassError] = useState<unknown>()
  const abort = useRef<AbortController | undefined>(undefined)
  useEffect(() => () => abort.current?.abort(), [])

  const choice = (token: string) => ({ token, includeStuck, includeDeleted })
  const run = (token: string, first: DrainProgress) => {
    abort.current?.abort()
    const controller = new AbortController()
    abort.current = controller
    setPassError(undefined)
    setRunning(true)
    drain({
      first,
      pass: () => evictNodePods(node, choice(token)),
      sleep,
      signal: controller.signal,
      onProgress: (next) => {
        if (controller.signal.aborted) return
        setProgress(next)
        refresh()
      },
    })
      .catch((error: unknown) => {
        if (!controller.signal.aborted) setPassError(error)
      })
      .finally(() => {
        if (!controller.signal.aborted) setRunning(false)
      })
  }
  const apply = useMutation({
    mutationFn: (token: string) => applyNodeMaintenance(node, choice(token)),
    onSuccess: (first, token) => {
      if (action === "remove") return onRemoved()
      refresh()
      if (action !== "drain") return onClose()
      setProgress(first)
      run(token, first)
    },
    onError: refresh,
  })
  const retry = useMutation({
    mutationFn: (token: string) => evictNodePods(node, choice(token)),
    onSuccess: (first, token) => {
      setProgress(first)
      refresh()
      run(token, first)
    },
  })

  const left = useSecondsLeft(progress ? undefined : plan.data?.expiresAt)
  const expired = left !== undefined && left <= 0
  const ready = plan.data && !plan.isFetching
  const split = fates(plan.data?.pods ?? [])
  const confirmNeeded =
    (action === "drain" && split.deleted.length > 0 && !includeDeleted) ||
    (action === "remove" && split.stuck.length > 0 && !loseData)
  const refused = plan.data?.removal?.refused
  const title = t(`maint.title.${action}`, node)

  if (progress && plan.data)
    return (
      <Frame
        title={title}
        wide
        onClose={onClose}
        footer={
          <div className="flex w-full flex-wrap items-center gap-2">
            {!running && !progress.done && !zeaburLeaving(progress) ? (
              <Button
                variant="outline"
                size="sm"
                loading={retry.isPending}
                onClick={() => plan.data && retry.mutate(plan.data.token)}
              >
                {t("maint.retry")}
              </Button>
            ) : null}
            <span className="ml-auto flex gap-2">
              <Button
                size="sm"
                variant={progress.done && !removing ? "default" : "ghost"}
                onClick={onClose}
              >
                {t("common.close")}
              </Button>
              {removing && progress.done ? (
                <Button size="sm" onClick={() => onAction("remove")}>
                  {t("maint.removeNext")}
                </Button>
              ) : null}
            </span>
          </div>
        }
      >
        <Progress
          node={node}
          progress={progress}
          running={running}
          kept={includeStuck ? 0 : split.stuck.length}
        />
        {passError ? <Problem error={passError} /> : null}
        {retry.isError ? <Problem error={retry.error} /> : null}
      </Frame>
    )

  return (
    <Frame
      title={title}
      wide={action === "drain"}
      onClose={onClose}
      footer={
        <div className="flex w-full flex-wrap items-center gap-2">
          <Button variant="ghost" size="sm" onClick={onClose}>
            {t("plan.cancel")}
          </Button>
          {ready && left !== undefined && left < COUNTDOWN_FROM_SECONDS ? (
            <span className="type-meta text-muted-foreground tabular-nums">
              {expired ? t("plan.expired") : t("plan.expiresIn", clock(left))}
            </span>
          ) : null}
          <span className="ml-auto">
            {expired ? (
              <Button size="sm" onClick={() => void plan.refetch()}>
                {t("plan.replan")}
              </Button>
            ) : refused === "notDrained" ? (
              <Button size="sm" onClick={() => onAction("drain")}>
                {t("maint.removal.drainFirst")}
              </Button>
            ) : refused ? null : (
              <Button
                size="sm"
                disabled={!ready || confirmNeeded}
                loading={apply.isPending}
                onClick={() => plan.data && apply.mutate(plan.data.token)}
              >
                {action === "remove" ? t("maint.remove") : t("plan.apply")}
              </Button>
            )}
          </span>
        </div>
      }
    >
      {plan.isPending || plan.isFetching ? (
        <LoadingRows label={t("plan.planning")} rows={3} />
      ) : plan.isError ? (
        <Problem error={plan.error} />
      ) : action === "drain" ? (
        <DrainPlan
          plan={plan.data}
          includeStuck={includeStuck}
          onIncludeStuck={setIncludeStuck}
          includeDeleted={includeDeleted}
          onIncludeDeleted={setIncludeDeleted}
        />
      ) : action === "remove" ? (
        <RemovePlan plan={plan.data} loseData={loseData} onLoseData={setLoseData} />
      ) : (
        <>
          <p>{t(`maint.lead.${action}`, node)}</p>
          <KubeDetails facts={[[t("maint.kube.command"), kubectlCommand(action, node, false)]]} />
        </>
      )}
      {apply.isError ? <Problem error={apply.error} /> : null}
    </Frame>
  )
}

function RemovePlan({
  plan,
  loseData,
  onLoseData,
}: {
  plan: MaintenancePlan
  loseData: boolean
  onLoseData: (value: boolean) => void
}) {
  const t = useT()
  const loseId = useId()
  const node = plan.node
  const removal = plan.removal
  if (!removal) return null
  const lost = fates(plan.pods).stuck.length
  const control = plan.roles.some((role) => ["control-plane", "master", "etcd"].includes(role))
  return (
    <>
      {removal.refused === "lastNode" ? (
        <Note variant="destructive">{t("maint.removal.lastNode", node)}</Note>
      ) : removal.refused === "lastServer" ? (
        <Note variant="destructive">{t("maint.removal.lastServer", node)}</Note>
      ) : removal.refused === "notDrained" ? (
        <p>{t("maint.removal.notDrained", node)}</p>
      ) : (
        <>
          <p>{t("maint.lead.remove", node)}</p>
          {!removal.ready && removal.left > 0 ? (
            <Note variant="warn">{t("maint.removal.down", node, removal.left)}</Note>
          ) : null}
          {lost > 0 ? (
            <Group
              health="fail"
              title={t("maint.removal.lose", lost, node)}
              groups={byProject(fates(plan.pods).stuck)}
            >
              <Choice id={loseId} checked={loseData} onChange={onLoseData}>
                {t("maint.removal.loseConfirm")}
              </Choice>
            </Group>
          ) : null}
          {control && removal.otherServers < 3 ? (
            <Note variant="warn">{t("maint.removal.servers", removal.otherServers)}</Note>
          ) : null}
        </>
      )}
      <KubeDetails facts={[[t("maint.kube.command"), kubectlCommand("remove", node, false)]]} />
    </>
  )
}

/** After a node is removed: what to run on its machine. */
export function Removed({ node, onClose }: { node: string; onClose: () => void }) {
  const t = useT()
  useEffect(() => {
    refresh()
    void queryClient.invalidateQueries({ queryKey: nodePoliciesQuery.queryKey })
  }, [])
  return (
    <Frame
      title={t("maint.title.remove", node)}
      onClose={onClose}
      footer={
        <span className="ml-auto">
          <Button size="sm" onClick={onClose}>
            {t("common.close")}
          </Button>
        </span>
      }
    >
      <p className="flex items-center gap-2" role="status">
        <StatusDot health="ok" />
        {t("maint.removed", node)}
      </p>
      <p>{t("maint.removed.run", node)}</p>
      <YamlBlock yaml={UNINSTALL_COMMANDS} label={t("maint.removed.commands", node)} />
      <p className="type-meta text-muted-foreground">{t("maint.removed.volumes")}</p>
    </Frame>
  )
}

function Note({
  variant,
  children,
}: {
  variant: "warn" | "destructive" | "info" | "default"
  children: ReactNode
}) {
  return (
    <Alert variant={variant} role="note">
      {variant === "info" ? <InfoIcon aria-hidden="true" /> : <WarningIcon aria-hidden="true" />}
      <AlertDescription>{children}</AlertDescription>
    </Alert>
  )
}

function DrainPlan({
  plan,
  includeStuck,
  onIncludeStuck,
  includeDeleted,
  onIncludeDeleted,
}: {
  plan: MaintenancePlan
  includeStuck: boolean
  onIncludeStuck: (value: boolean) => void
  includeDeleted: boolean
  onIncludeDeleted: (value: boolean) => void
}) {
  const t = useT()
  const { cores, memory } = useQuantityFormat()
  const stuckId = useId()
  const deletedId = useId()
  const node = plan.node
  const split = fates(plan.pods)
  const moving = split.moves.length + (includeStuck ? split.stuck.length : 0)
  const scratch = split.moves.filter((pod) => pod.scratch.length > 0).length
  const amounts = (r: Schemas["Resources"]) =>
    t("maint.amounts", t("cluster.cores", cores(r.cpuMillicores)), memory(r.memoryBytes))
  const capacity = plan.capacity
  const control = plan.roles.some((role) => ["control-plane", "master", "etcd"].includes(role))
  const budgets = [...new Set(plan.pods.flatMap((pod) => (pod.budget ? [pod.budget] : [])))]
  return (
    <>
      <p>
        {plan.cordoned
          ? t("maint.lead.continue", node, moving)
          : t("maint.lead.drain", node, moving)}
      </p>
      {plan.otherSchedulable === 0 && split.moves.length > 0 ? (
        <Note variant="destructive">{t("maint.noOther")}</Note>
      ) : capacity?.status === "short" ? (
        <Note variant="warn">
          {t("maint.short", capacity.unplaced.length)}{" "}
          {capacity.room
            ? t("maint.shortDetail", amounts(capacity.needs), amounts(capacity.room))
            : null}
        </Note>
      ) : null}
      {control ? <Note variant="warn">{t("maint.controlPlane", node)}</Note> : null}
      {plan.zeaburHere ? <Note variant="info">{t("maint.zeabur")}</Note> : null}
      {plan.unreadable.length > 0 ? <Note variant="default">{t("maint.unreadable")}</Note> : null}

      {split.moves.length > 0 ? (
        <Group
          title={t("maint.moves", split.moves.length)}
          note={
            capacity?.status === "fits" && capacity.room ? (
              <span className="flex items-center gap-2">
                <StatusDot health="ok" />
                {t("maint.fits", amounts(capacity.needs), amounts(capacity.room))}
              </span>
            ) : capacity?.status === "unknown" ? (
              t("maint.unknownRoom")
            ) : undefined
          }
          extra={
            scratch > 0 ? (
              <span className="text-warn-strong">{t("maint.scratch", scratch)}</span>
            ) : undefined
          }
          groups={byProject(split.moves)}
        />
      ) : null}
      {split.stuck.length > 0 ? (
        <Group
          health="fail"
          title={t("maint.stuck", split.stuck.length)}
          note={t("maint.stuck.why", node)}
          groups={byProject(split.stuck)}
        >
          <Choice id={stuckId} checked={includeStuck} onChange={onIncludeStuck}>
            {t("maint.stuck.include", node)}
          </Choice>
        </Group>
      ) : null}
      {split.deleted.length > 0 ? (
        <Group
          health="warn"
          title={t("maint.deleted", split.deleted.length)}
          note={t("maint.deleted.why")}
          groups={byProject(split.deleted)}
        >
          <Choice id={deletedId} checked={includeDeleted} onChange={onIncludeDeleted}>
            {t("maint.deleted.confirm")}
          </Choice>
        </Group>
      ) : null}
      {split.stays.length > 0 ? (
        <Group
          health="idle"
          title={t("maint.stays", split.stays.length)}
          note={t("maint.stays.why")}
          groups={byProject(split.stays)}
          muted
        />
      ) : null}
      <KubeDetails
        facts={[
          [t("maint.kube.command"), kubectlCommand("drain", node, includeDeleted)],
          ...(budgets.length > 0
            ? [[t("maint.kube.budgets"), budgets.join(", ")] as [string, string]]
            : []),
        ]}
      />
    </>
  )
}

function Choice({
  id,
  checked,
  onChange,
  children,
}: {
  id: string
  checked: boolean
  onChange: (value: boolean) => void
  children: ReactNode
}) {
  return (
    <div className="flex items-start gap-2.5 pt-1">
      <Checkbox
        id={id}
        checked={checked}
        onCheckedChange={(value) => onChange(value)}
        className="mt-0.5"
      />
      <Label htmlFor={id}>{children}</Label>
    </div>
  )
}

const headingTone: Partial<Record<Health, string>> = {
  fail: "text-fail-strong",
  warn: "text-warn-strong",
}

function Group({
  title,
  health,
  note,
  extra,
  groups,
  muted,
  children,
}: {
  title: string
  health?: Health
  note?: ReactNode
  extra?: ReactNode
  groups: PodGroup[]
  muted?: boolean
  children?: ReactNode
}) {
  const t = useT()
  return (
    <section aria-label={title} className="flex flex-col gap-1.5 border-t border-rule pt-3">
      <h3 className={cn("flex items-center gap-2 font-medium", health && headingTone[health])}>
        {health ? <StatusDot health={health} /> : null}
        {title}
      </h3>
      {note ? <p className="type-meta text-muted-foreground">{note}</p> : null}
      {extra ? <p className="type-meta">{extra}</p> : null}
      <ul className="flex flex-col gap-1">
        {groups.map((group) => (
          <li key={group.key} className="grid gap-x-4 gap-y-0.5 sm:grid-cols-drain">
            <span className="truncate" translate="no">
              {group.label}
            </span>
            <span
              className={cn(
                "flex min-w-0 flex-wrap gap-x-3 gap-y-0.5",
                muted && "text-muted-foreground",
              )}
              translate="no"
            >
              {group.workloads.map((workload) => (
                <span key={workload.name} className="min-w-0 break-all">
                  {workload.name}
                  {workload.count > 1 ? (
                    <span className="text-muted-foreground"> ×{workload.count}</span>
                  ) : null}
                  {workload.scratch ? (
                    <span className="type-meta text-warn-strong" translate="yes">
                      {" "}
                      {t("maint.lose")}
                    </span>
                  ) : null}
                  {workload.budget ? (
                    <span className="type-meta text-warn-strong" translate="yes">
                      {" "}
                      {t("maint.budget", workload.budget)}
                    </span>
                  ) : null}
                </span>
              ))}
              {group.pods.some((pod) => pod.stays) ? (
                <span className="type-meta" translate="yes">
                  {[...new Set(group.pods.flatMap((pod) => (pod.stays ? [pod.stays] : [])))]
                    .map((reason) => t(`maint.reason.${reason}`))
                    .join(t("common.sep"))}
                </span>
              ) : null}
            </span>
          </li>
        ))}
      </ul>
      {children}
    </section>
  )
}

const outcomeTone: Record<Schemas["EvictOutcome"], string | undefined> = {
  gone: undefined,
  leaving: "text-muted-foreground",
  waiting: "text-warn-strong",
  refused: "text-fail-strong",
  failed: "text-fail-strong",
  last: "text-muted-foreground",
}

function Progress({
  node,
  progress,
  running,
  kept,
}: {
  node: string
  progress: DrainProgress
  running: boolean
  kept: number
}) {
  const t = useT()
  const held = progress.pods.filter((pod) => pod.outcome !== "gone" && pod.outcome !== "leaving")
  const moving = !running && !progress.done && zeaburLeaving(progress)
  const stuck = !running && !progress.done && !moving
  const health: Health = progress.done ? "idle" : stuck ? "fail" : "progress"
  return (
    <>
      <p className="flex items-center gap-2" role="status">
        <StatusDot health={health} />
        {progress.done
          ? t("maint.done", node, kept)
          : moving
            ? t("maint.zeaburMoving", node)
            : stuck
              ? t("maint.stopped")
              : t("maint.progress", node, progress.remaining, progress.pods.length)}
      </p>
      {held.length > 0 ? (
        <ul className="flex flex-col gap-1">
          {held.map((pod) => (
            <li
              key={`${pod.namespace}/${pod.name}`}
              className="grid gap-x-4 gap-y-0.5 sm:grid-cols-evicted"
            >
              <span className="min-w-0 truncate" translate="no">
                <span className="text-muted-foreground">{pod.namespace}/</span>
                {pod.name}
              </span>
              <span className={cn("type-meta", outcomeTone[pod.outcome])}>
                {pod.outcome === "waiting"
                  ? t("maint.outcome.waiting", pod.budget ?? undefined)
                  : t(`maint.outcome.${pod.outcome as "refused" | "failed" | "last"}`)}
                {pod.detail ? (
                  <span className="block text-muted-foreground" translate="no">
                    {pod.detail}
                  </span>
                ) : null}
              </span>
            </li>
          ))}
        </ul>
      ) : null}
      {!progress.done && !moving ? (
        <p className="type-meta text-muted-foreground">{t("maint.leaveNote")}</p>
      ) : null}
    </>
  )
}
