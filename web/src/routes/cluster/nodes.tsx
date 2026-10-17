// Nodes and capacity: what the pods request against what each node can give them (allocatable),
// limits as a tick, and usage as a second, thinner bar when metrics-server is installed.
import { useQuery } from "@tanstack/react-query"
import type { CSSProperties } from "react"

import { isStatus, type Schemas } from "@/api/client"
import { nodesQuery, upgradesQuery } from "@/api/queries"
import { LoadingRows } from "@/components/page"
import { Problem } from "@/components/problem"
import { StatusPill } from "@/components/status"
import { Tooltip } from "@/components/ui/tooltip"
import { useT, type Translate } from "@/i18n"
import { useQuantityFormat } from "@/lib/quantity"
import { cn } from "@/lib/utils"
import type { Health } from "@/model/service"
import { Forbidden, overviewRole } from "@/routes/cluster/forbidden"
import { Section } from "@/routes/cluster/section"

type Resources = Schemas["Resources"]
type NodeView = Schemas["NodeView"]

/** Requests above this share of allocatable need attention. */
export const TIGHT = 0.9

export const share = (part: number | undefined, whole: number) =>
  whole > 0 && part !== undefined ? part / whole : undefined

/** A node's state in one word, worst first: not ready, a pressure, cordoned, ready. */
export function nodeState(node: NodeView, t: Translate): { health: Health; word: string } {
  if (node.ready !== "True")
    return {
      health: "fail",
      word: node.ready === "Unknown" ? t("status.unknown") : t("cluster.notReady"),
    }
  const pressure = node.problems.find((p) => p.type !== "Ready")
  if (pressure) return { health: "warn", word: pressure.type }
  if (node.cordoned) return { health: "warn", word: t("cluster.cordoned") }
  return { health: "ok", word: t("status.ready") }
}

/** Where a share falls on the bar, as a CSS length (0–100%). */
const at = (ratio: number | undefined) => `${Math.min(Math.max(ratio ?? 0, 0), 1) * 100}%`

/** Requests as the bar, limits as a tick (may pass the end: overcommitted), usage as a thin bar
 * below. Positions are custom properties the classes read. */
function CapacityBar({
  requested,
  limit,
  used,
  label,
  size = "md",
}: {
  requested?: number
  limit?: number
  used?: number
  label: string
  size?: "sm" | "md"
}) {
  const tight = (requested ?? 0) >= TIGHT
  const style = {
    "--requested": at(requested),
    "--limit": at(limit),
    "--used": at(used),
  } as CSSProperties
  return (
    <div role="img" aria-label={label} className="flex flex-col gap-0.5" style={style}>
      <div
        className={cn(
          "relative overflow-hidden rounded-full bg-foreground/10",
          size === "md" ? "h-2" : "h-1.5",
        )}
      >
        {requested !== undefined ? (
          <div
            className={cn(
              "absolute inset-y-0 left-0 w-(--requested) rounded-full",
              tight ? "bg-warn" : "bg-foreground/50",
            )}
          />
        ) : null}
        {limit !== undefined && limit > 0 && limit < 1 ? (
          <div className="absolute inset-y-0 left-(--limit) w-px bg-foreground" />
        ) : null}
      </div>
      {used !== undefined ? (
        <div className="relative h-1 overflow-hidden rounded-full bg-foreground/10">
          <div className="absolute inset-y-0 left-0 w-(--used) rounded-full bg-progress" />
        </div>
      ) : null}
    </div>
  )
}

function Total({
  title,
  allocatable,
  requests,
  limits,
  usage,
  format,
  describe,
}: {
  title: string
  allocatable: number
  requests?: number
  limits?: number
  usage?: number
  format: (value: number) => string
  /** "1.2 of 2 cores requested"; pods say so their own way. */
  describe?: (requests: string, allocatable: string) => string
}) {
  const t = useT()
  const { percent } = useQuantityFormat()
  const say = describe ?? ((r: string, a: string) => t("cluster.requestedOf", r, a))
  const requested = share(requests, allocatable)
  const used = share(usage, allocatable)
  const label = [
    title,
    requests !== undefined ? say(format(requests), format(allocatable)) : "",
    used !== undefined && usage !== undefined ? t("cluster.used", format(usage)) : "",
  ]
    .filter(Boolean)
    .join(", ")
  return (
    <div className="flex min-w-0 flex-1 basis-56 flex-col gap-2">
      <div className="flex items-baseline justify-between gap-2">
        <span className="type-meta text-muted-foreground">{title}</span>
        {requested !== undefined ? (
          <span className={cn("type-figure", requested >= TIGHT && "text-warn-strong")}>
            {percent(requested)}
          </span>
        ) : null}
      </div>
      <CapacityBar
        requested={requested}
        limit={share(limits, allocatable)}
        used={used}
        label={label}
      />
      <span className="type-meta text-muted-foreground">
        {requests !== undefined
          ? say(format(requests), format(allocatable))
          : t("cluster.allocatable", format(allocatable))}
        {usage !== undefined ? `${t("common.sep")}${t("cluster.used", format(usage))}` : ""}
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
        isStatus(nodes.error, 403) ? (
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
          <div className="flex flex-wrap gap-x-8 gap-y-5 rounded-lg bg-plane px-4 py-4">
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
            />
            <Total
              title={t("pod.pods")}
              allocatable={data.totals.allocatable.pods ?? 0}
              requests={data.totals.requests?.pods ?? undefined}
              format={plain}
              describe={(placed, room) => t("cluster.podsOf", placed, room)}
            />
          </div>
          <Notes data={data} />
          <ul aria-label={t("cluster.nodes")} className="flex flex-col">
            {data.items.map((node) => (
              <NodeRow
                key={node.id}
                node={node}
                skew={skew.find((s) => s.node === node.name)}
                metrics={data.metrics}
              />
            ))}
          </ul>
        </div>
      ) : null}
    </Section>
  )
}

function Notes({ data }: { data: Schemas["NodeList"] }) {
  const t = useT()
  const lines: string[] = []
  if (data.metrics === "not-installed") lines.push(t("cluster.noMetrics"))
  if (data.metrics === "unavailable") lines.push(t("cluster.metricsUnavailable"))
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

function NodeRow({
  node,
  skew,
  metrics,
}: {
  node: NodeView
  skew?: Schemas["NodeSkew"]
  metrics: Schemas["MetricsState"]
}) {
  const t = useT()
  const { cores, memory, percent } = useQuantityFormat()
  const state = nodeState(node, t)
  const pair = (title: string, pick: (r: Resources) => number, format: (n: number) => string) => {
    const whole = pick(node.allocatable)
    const requests = node.requests ? pick(node.requests) : undefined
    const used = node.usage ? pick(node.usage) : undefined
    const requested = share(requests, whole)
    return (
      <div className="flex min-w-0 flex-col gap-1 sm:w-40">
        <div className="flex items-baseline justify-between gap-2 type-meta">
          <span className="text-muted-foreground">{title}</span>
          <span className={cn(requested !== undefined && requested >= TIGHT && "text-warn-strong")}>
            {requested !== undefined ? percent(requested) : "—"}
          </span>
        </div>
        <CapacityBar
          size="sm"
          requested={requested}
          limit={share(node.limits ? pick(node.limits) : undefined, whole)}
          used={share(used, whole)}
          label={[
            title,
            requests !== undefined ? t("cluster.requestedOf", format(requests), format(whole)) : "",
            used !== undefined ? t("cluster.used", format(used)) : "",
          ]
            .filter(Boolean)
            .join(", ")}
        />
      </div>
    )
  }
  const taints = node.taints.map((taint) => `${taint.key}:${taint.effect}`).join("\n")
  return (
    <li className="flex flex-wrap items-center gap-x-6 gap-y-3 border-t border-rule py-3">
      <div className="flex min-w-56 flex-1 flex-col gap-1">
        <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1">
          <span className="font-semibold" translate="no">
            {node.name}
          </span>
          {taints ? (
            <Tooltip content={<span className="whitespace-pre-line">{taints}</span>}>
              <span>
                <StatusPill health={state.health} size="sm">
                  {state.word}
                </StatusPill>
              </span>
            </Tooltip>
          ) : (
            <StatusPill health={state.health} size="sm">
              {state.word}
            </StatusPill>
          )}
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
      </div>
      <div className="grid w-full grid-cols-3 items-center gap-x-4 sm:flex sm:w-auto sm:gap-x-6">
        {pair(
          t("cluster.cpu"),
          (r) => r.cpuMillicores,
          (m) => t("cluster.cores", cores(m)),
        )}
        {pair(t("cluster.memory"), (r) => r.memoryBytes, memory)}
        <div className="flex flex-col gap-1 type-meta sm:w-16">
          <span className="text-muted-foreground">{t("pod.pods")}</span>
          <span>
            {node.requests?.pods ?? "—"}
            <span className="text-muted-foreground">/{node.allocatable.pods ?? "—"}</span>
          </span>
        </div>
      </div>
      {metrics === "available" && !node.usage ? (
        <span className="sr-only">{t("cluster.noUsageYet")}</span>
      ) : null}
    </li>
  )
}
