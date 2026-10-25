import { useQuery } from "@tanstack/react-query"
import { useState } from "react"
import CaretDownIcon from "~icons/ph/caret-down"
import MagnifyingGlassIcon from "~icons/ph/magnifying-glass"

import { isStatus, isUnreachable, type Schemas } from "@/api/client"
import { nodesQuery, upgradesQuery } from "@/api/queries"
import { Segmented } from "@/components/bits"
import { LoadingRows } from "@/components/page"
import { Problem } from "@/components/problem"
import { StatusDot, StatusPill } from "@/components/status"
import { NotRead } from "@/components/unreachable"
import { NodeShells } from "@/components/node-shell"
import { Button } from "@zeabur/design-system/components/button"
import { Tooltip } from "@zeabur/design-system/components/tooltip"
import { useT } from "@/i18n"
import { useQuantityFormat } from "@/lib/quantity"
import { cn } from "@/lib/utils"
import {
  byNodeSeverity,
  DISK,
  diskShare,
  fillHealth,
  MEMORY,
  memoryUsed,
  nodeIssues,
  share,
  TIGHT,
  worstNode,
  type IssueKind,
} from "@/model/node-health"
import type { Health } from "@/model/service"
import { useIssueText } from "@/routes/cluster/attention"
import { Forbidden, overviewRole } from "@/routes/cluster/forbidden"
import {
  ContinueMaintenance,
  NodeDialog,
  NodeMenu,
  type NodeOpen,
  Removed,
} from "@/routes/cluster/node-maintenance"
import { Section } from "@/routes/cluster/section"
import { Meter } from "@zeabur/design-system/components/meter"

type Resources = Schemas["Resources"]
type NodeView = Schemas["NodeView"]

/** Above this many nodes the list gets a filter, a sort and a bound. */
const LIST_AT = 7
const ROWS_SHOWN = 20

const meterTone = { ok: "ok", warn: "warn", fail: "over", progress: "ok", idle: "muted" } as const
const figureTone = (health: Health) =>
  health === "fail" ? "text-fail-strong" : health === "warn" ? "text-warn-strong" : undefined

/** What leads a bar: measured use when there is one, coloured by how full it is; else what pods
 * reserve, which only says whether new pods fit. */
function fill({
  allocatable,
  requests,
  usage,
  limits,
}: {
  allocatable: number
  requests?: number
  usage?: number
  limits?: { warn: number; fail: number }
}) {
  const used = share(usage, allocatable)
  const reserved = share(requests, allocatable)
  if (used !== undefined)
    return {
      lead: used,
      mark: reserved,
      reserved,
      health: limits ? fillHealth(used, limits) : ("ok" as Health),
    }
  return {
    lead: reserved,
    mark: undefined,
    reserved: undefined,
    health: (reserved ?? 0) >= TIGHT ? ("warn" as Health) : ("ok" as Health),
  }
}

function Total({
  title,
  allocatable,
  requests,
  limits,
  usage,
  format,
  thresholds,
  describe,
}: {
  title: string
  allocatable: number
  requests?: number
  limits?: number
  usage?: number
  format: (value: number) => string
  thresholds?: { warn: number; fail: number }
  describe?: (requests: string, allocatable: string) => string
}) {
  const t = useT()
  const { percent } = useQuantityFormat()
  const say = describe ?? ((r: string, a: string) => t("cluster.requestedOf", r, a))
  const bar = fill({ allocatable, requests, usage, limits: thresholds })
  const label = [
    title,
    usage !== undefined ? t("cluster.usedOf", format(usage), format(allocatable)) : "",
    requests !== undefined ? say(format(requests), format(allocatable)) : "",
    limits ? t("cluster.limits", format(limits)) : "",
  ]
    .filter(Boolean)
    .join(", ")
  return (
    <div className="flex min-w-0 flex-1 basis-44 flex-col gap-2">
      <div className="flex items-baseline justify-between gap-2">
        <span className="type-meta text-muted-foreground">{title}</span>
        {bar.lead !== undefined ? (
          <span className={cn("type-figure", figureTone(bar.health))}>{percent(bar.lead)}</span>
        ) : null}
      </div>
      <Meter value={bar.lead} mark={bar.mark} tone={meterTone[bar.health]} label={label} />
      <span className="type-meta text-muted-foreground">
        {usage !== undefined
          ? t("cluster.usedOf", format(usage), format(allocatable))
          : requests !== undefined
            ? say(format(requests), format(allocatable))
            : t("cluster.allocatable", format(allocatable))}
        {bar.reserved !== undefined ? (
          <>
            {t("common.sep")}
            <span className={cn(bar.reserved >= TIGHT && "text-warn-strong")}>
              {t("cluster.reservedShare", percent(bar.reserved))}
            </span>
          </>
        ) : null}
      </span>
    </div>
  )
}

export function NodesSection() {
  const t = useT()
  const nodes = useQuery(nodesQuery)
  const skew = useQuery(upgradesQuery).data?.skew ?? []
  const { cores, memory } = useQuantityFormat()
  const plain = (n: number) => String(n)
  const data = nodes.data
  return (
    <Section
      id="cluster-nodes"
      title={t("cluster.nodes")}
      count={data ? data.items.length : undefined}
    >
      {nodes.isPending ? (
        <LoadingRows label={t("common.loading")} rows={2} />
      ) : nodes.isError ? (
        isUnreachable(nodes.error) ? (
          <NotRead />
        ) : isStatus(nodes.error, 403) ? (
          <Forbidden
            error={nodes.error}
            what={t("cluster.whatNodes")}
            missing={[{ group: "", resources: ["nodes"] }]}
            role={overviewRole}
          />
        ) : (
          <Problem error={nodes.error} />
        )
      ) : data ? (
        <div className="flex flex-col gap-5">
          <div className="flex flex-col gap-4 rounded-lg bg-plane px-4 py-4">
            <div className="flex flex-wrap gap-x-8 gap-y-5">
              <Total
                title={t("cluster.cpu")}
                allocatable={data.totals.allocatable.cpuMillicores}
                requests={data.totals.requests?.cpuMillicores}
                limits={data.totals.limits?.cpuMillicores}
                usage={data.totals.usage?.cpuMillicores}
                format={(m) => t("cluster.cores", cores(m))}
              />
              <Total
                title={t("cluster.memory")}
                allocatable={data.totals.allocatable.memoryBytes}
                requests={data.totals.requests?.memoryBytes}
                limits={data.totals.limits?.memoryBytes}
                usage={data.totals.usage?.memoryBytes}
                format={memory}
                thresholds={MEMORY}
              />
              {data.totals.disk ? (
                <Total
                  title={t("cluster.disk")}
                  allocatable={data.totals.disk.capacityBytes}
                  usage={data.totals.disk.capacityBytes - data.totals.disk.availableBytes}
                  format={memory}
                  thresholds={DISK}
                />
              ) : null}
              <Total
                title={t("cluster.pods")}
                allocatable={data.totals.allocatable.pods ?? 0}
                requests={data.totals.requests?.pods ?? undefined}
                format={plain}
                describe={(placed, room) => t("cluster.podsOf", placed, room)}
              />
            </div>
            <Worst nodes={data.items} />
          </div>
          <Notes data={data} />
          <NodeList nodes={data.items} skew={skew} metrics={data.metrics} />
        </div>
      ) : null}
    </Section>
  )
}

/** The totals average the nodes; the one that is in trouble is named under them. */
function Worst({ nodes }: { nodes: readonly NodeView[] }) {
  const issueText = useIssueText()
  const worst = worstNode(nodes)
  if (!worst) return null
  const { text } = issueText({
    kind: worst.issue.kind,
    health: worst.issue.health,
    issues: [worst.issue],
  })
  return (
    <a
      href={`#node-${worst.node.name}`}
      className="flex items-center gap-2 border-t border-rule pt-3 type-meta hover:underline"
    >
      <StatusDot health={worst.issue.health} />
      {text}
    </a>
  )
}

type Sort = "worst" | "name" | "memory" | "disk"

const sorters: Record<Sort, (a: NodeView, b: NodeView) => number> = {
  worst: byNodeSeverity,
  name: (a, b) => a.name.localeCompare(b.name),
  memory: (a, b) => (memoryUsed(b) ?? -1) - (memoryUsed(a) ?? -1) || a.name.localeCompare(b.name),
  disk: (a, b) =>
    (diskShare(b.disk) ?? -1) - (diskShare(a.disk) ?? -1) || a.name.localeCompare(b.name),
}

function NodeList({
  nodes,
  skew,
  metrics,
}: {
  nodes: readonly NodeView[]
  skew: readonly Schemas["NodeSkew"][]
  metrics: Schemas["MetricsState"]
}) {
  const t = useT()
  const [query, setQuery] = useState("")
  const [sort, setSort] = useState<Sort>("worst")
  const [all, setAll] = useState(false)
  const [removed, setRemoved] = useState<string>()
  const large = nodes.length > LIST_AT
  const needle = query.trim().toLowerCase()
  const matching = needle
    ? nodes.filter(
        (node) =>
          node.name.toLowerCase().includes(needle) ||
          node.roles.some((role) => role.toLowerCase().includes(needle)),
      )
    : nodes
  const sorted = matching.toSorted(sorters[sort])
  const shown = large && !all ? sorted.slice(0, ROWS_SHOWN) : sorted
  return (
    <div className="flex flex-col gap-3">
      {large ? (
        <div className="flex flex-wrap items-center gap-2">
          <Segmented
            label={t("cluster.sortNodes")}
            value={sort}
            onChange={setSort}
            options={[
              { value: "worst", label: t("cluster.sortWorst") },
              { value: "name", label: t("cluster.sortName") },
              { value: "memory", label: t("cluster.memory") },
              { value: "disk", label: t("cluster.disk") },
            ]}
          />
          <label className="flex h-8 w-full items-center gap-2 rounded-lg border bg-background px-2.5 sm:ml-auto sm:w-64">
            <MagnifyingGlassIcon className="size-4 text-muted-foreground" aria-hidden="true" />
            <input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              aria-label={t("cluster.filterNodes")}
              placeholder={t("cluster.filterNodes")}
              className="w-full bg-transparent text-sm outline-none placeholder:text-muted-foreground"
            />
          </label>
        </div>
      ) : null}
      {needle && matching.length === 0 ? (
        <p className="type-meta text-muted-foreground">{t("cluster.noNodeMatch", query.trim())}</p>
      ) : (
        <ul aria-label={t("cluster.nodes")} className="flex flex-col">
          {shown.map((node) => (
            <NodeRow
              key={node.id}
              node={node}
              skew={skew.find((s) => s.node === node.name)}
              metrics={metrics}
              onRemoved={setRemoved}
            />
          ))}
        </ul>
      )}
      <Removed node={removed} onClose={() => setRemoved(undefined)} />
      {large && sorted.length > ROWS_SHOWN ? (
        <div>
          <Button variant="ghost" size="sm" onClick={() => setAll(!all)} aria-expanded={all}>
            <CaretDownIcon aria-hidden="true" data-icon="inline-start" />
            {all ? t("cluster.fewer") : t("cluster.more", sorted.length - ROWS_SHOWN)}
          </Button>
        </div>
      ) : null}
    </div>
  )
}

function Notes({ data }: { data: Schemas["NodeList"] }) {
  const t = useT()
  const lines: string[] = []
  if (data.metrics === "not-installed") lines.push(t("cluster.noMetrics"))
  if (data.metrics === "unavailable") lines.push(t("cluster.metricsUnavailable"))
  if (data.disk !== "available") lines.push(t(`cluster.diskState.${data.disk}`))
  if (data.podScan === "partial") lines.push(t("cluster.partialPods", data.podsScanned))
  if (data.unscheduledPods > 0) lines.push(t("cluster.unscheduled", data.unscheduledPods))
  const podsRefused = data.podScan === "unreadable"
  const metricsRefused = data.metrics === "unreadable"
  return (
    <>
      {lines.length > 0 ? (
        <ul className="flex flex-col gap-1 type-meta text-muted-foreground">
          {lines.map((line) => (
            <li key={line}>{line}</li>
          ))}
        </ul>
      ) : null}
      {podsRefused || metricsRefused ? (
        <Forbidden
          what={podsRefused ? t("cluster.whatRequests") : t("cluster.whatUsage")}
          missing={[
            ...(podsRefused ? [{ group: "", resources: ["pods"] }] : []),
            ...(metricsRefused ? [{ group: "metrics.k8s.io", resources: ["nodes"] }] : []),
          ]}
          role={overviewRole}
        />
      ) : null}
    </>
  )
}

const quiet = new Set<IssueKind>(["reservedMemory", "reservedCpu"])

/** What is wrong with a node; the reservations show in the bars. */
function NodePills({ node }: { node: NodeView }) {
  const t = useT()
  const issues = nodeIssues(node).filter((issue) => !quiet.has(issue.kind))
  const taints = node.taints.map((taint) => `${taint.key}:${taint.effect}`).join("\n")
  const pills =
    issues.length === 0 ? (
      <StatusPill health="ok" size="sm">
        {t("status.ready")}
      </StatusPill>
    ) : (
      issues.map((issue) => (
        <StatusPill key={issue.kind} health={issue.health} size="sm">
          {issue.kind === "notReady" && node.ready === "Unknown"
            ? t("status.unknown")
            : issue.kind === "draining"
              ? t("node.drainingLeft", node.movablePods ?? 0)
              : t(`node.word.${issue.kind}`)}
        </StatusPill>
      ))
    )
  return taints ? (
    <Tooltip content={<span className="whitespace-pre-line">{taints}</span>}>
      <span className="flex flex-wrap gap-1">{pills}</span>
    </Tooltip>
  ) : (
    pills
  )
}

function NodeRow({
  node,
  skew,
  metrics,
  onRemoved,
}: {
  node: NodeView
  skew?: Schemas["NodeSkew"]
  metrics: Schemas["MetricsState"]
  onRemoved: (node: string) => void
}) {
  const t = useT()
  const { cores, memory, percent } = useQuantityFormat()
  const [open, setOpen] = useState<NodeOpen>()
  const gauge = (
    title: string,
    allocatable: number,
    format: (n: number) => string,
    numbers: {
      requests?: number
      usage?: number
      maximum?: number
      thresholds?: { warn: number; fail: number }
    },
  ) => {
    const bar = fill({
      allocatable,
      requests: numbers.requests,
      usage: numbers.usage,
      limits: numbers.thresholds,
    })
    return (
      <div className="flex min-w-0 flex-col gap-1 sm:w-36">
        <div className="flex items-baseline justify-between gap-2 type-meta">
          <span className="text-muted-foreground">{title}</span>
          <span className={figureTone(bar.health)}>
            {bar.lead !== undefined ? (
              percent(bar.lead)
            ) : (
              <span className="text-muted-foreground">—</span>
            )}
          </span>
        </div>
        <Meter
          size="sm"
          value={bar.lead}
          mark={bar.mark}
          tone={meterTone[bar.health]}
          label={[
            title,
            numbers.usage !== undefined
              ? t("cluster.usedOf", format(numbers.usage), format(allocatable))
              : "",
            numbers.requests !== undefined
              ? t("cluster.requestedOf", format(numbers.requests), format(allocatable))
              : "",
            numbers.maximum ? t("cluster.limits", format(numbers.maximum)) : "",
          ]
            .filter(Boolean)
            .join(", ")}
        />
        {bar.reserved !== undefined ? (
          <span
            className={cn(
              "type-meta text-muted-foreground",
              bar.reserved >= TIGHT && "text-warn-strong",
            )}
          >
            {t("cluster.reservedShare", percent(bar.reserved))}
          </span>
        ) : null}
      </div>
    )
  }
  const pick = (r: Resources | null | undefined, of: (r: Resources) => number) =>
    r ? of(r) : undefined
  const cpu = (r: Resources) => r.cpuMillicores
  const mem = (r: Resources) => r.memoryBytes
  return (
    <li
      id={`node-${node.name}`}
      className="flex scroll-mt-20 flex-wrap items-center gap-x-6 gap-y-3 border-t border-rule py-3"
    >
      <div className="flex min-w-56 flex-1 flex-col gap-1">
        <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1">
          <span className="font-semibold" translate="no">
            {node.name}
          </span>
          <NodePills node={node} />
          <span className="type-meta text-muted-foreground">
            {node.roles.length > 0 ? node.roles.join(", ") : t("cluster.worker")}
          </span>
        </div>
        <div className="flex flex-wrap items-center gap-x-2 type-meta text-muted-foreground">
          <Tooltip
            content={
              skew
                ? skew.unsupported
                  ? t("cluster.skewUnsupported")
                  : t("cluster.skewMixed")
                : t("cluster.kubelet")
            }
          >
            <span
              className={cn(
                "type-mono",
                skew && (skew.unsupported ? "text-fail-strong" : "text-warn-strong"),
              )}
              translate="no"
            >
              {node.kubeletVersion}
            </span>
          </Tooltip>
          <Tooltip
            content={[node.osImage, node.kernelVersion, node.containerRuntime].join(
              t("common.sep"),
            )}
          >
            <span translate="no">{node.architecture}</span>
          </Tooltip>
        </div>
        {node.cordoned && (node.pinnedPods ?? 0) > 0 ? (
          <p className="type-meta text-muted-foreground">
            {t("node.pinnedLeft", node.pinnedPods ?? 0)}
          </p>
        ) : null}
        <ContinueMaintenance node={node} onOpen={setOpen} />
      </div>
      <NodeMenu
        node={node}
        className="-my-1 self-start sm:order-last sm:self-center"
        onOpen={setOpen}
      />
      <NodeDialog
        node={node.name}
        open={open}
        onOpen={setOpen}
        onRemoved={onRemoved}
        onClose={() => setOpen(undefined)}
      />
      <div className="grid w-full grid-cols-2 items-start gap-x-4 gap-y-3 sm:flex sm:w-auto sm:gap-x-6">
        {gauge(
          t("cluster.cpu"),
          node.allocatable.cpuMillicores,
          (m) => t("cluster.cores", cores(m)),
          {
            requests: pick(node.requests, cpu),
            usage: pick(node.usage, cpu),
            maximum: pick(node.limits, cpu),
          },
        )}
        {gauge(t("cluster.memory"), node.allocatable.memoryBytes, memory, {
          requests: pick(node.requests, mem),
          usage: pick(node.usage, mem),
          maximum: pick(node.limits, mem),
          thresholds: MEMORY,
        })}
        {node.disk
          ? gauge(t("cluster.disk"), node.disk.capacityBytes, memory, {
              usage: node.disk.capacityBytes - node.disk.availableBytes,
              thresholds: DISK,
            })
          : null}
        <div className="flex flex-col gap-1 type-meta sm:w-16">
          <span className="text-muted-foreground">{t("pod.pods")}</span>
          <span>
            {node.requests?.pods ?? <span className="text-muted-foreground">—</span>}
            <span className="text-muted-foreground">/{node.allocatable.pods ?? "—"}</span>
          </span>
        </div>
      </div>
      <NodeShells node={node.name} />
      {metrics === "available" && !node.usage ? (
        <span className="sr-only">{t("cluster.noUsageYet")}</span>
      ) : null}
    </li>
  )
}
