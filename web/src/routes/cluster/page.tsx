// The cluster's Overview tab: the cluster itself, not its projects (Home has those). What needs
// attention, the nodes and their capacity, and the last hour's warnings about the cluster (no
// namespace, a Node, or a kube- namespace). Each section reads its own endpoint and shows its own
// loading, empty and permission state.
import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { useState, type ReactNode } from "react"
import CaretDownIcon from "~icons/ph/caret-down"

import { isStatus, type Schemas } from "@/api/client"
import {
  capabilitiesQuery,
  clusterGapsQuery,
  clusterWarningsQuery,
  nodesQuery,
  upgradesQuery,
} from "@/api/queries"
import { Ago } from "@/components/bits"
import { GapList } from "@/components/gaps"
import { LoadingRows } from "@/components/page"
import { Problem } from "@/components/problem"
import { StatusDot } from "@/components/status"
import { Button } from "@zeabur/design-system/components/button"
import { Card, CardContent, CardHeader, CardTitle } from "@zeabur/design-system/components/card"
import { useT } from "@/i18n"
import { useQuantityFormat } from "@/lib/quantity"
import { healthRank, type Health } from "@/model/service"
import { ClusterFrame } from "@/routes/cluster/frame"
import { Forbidden, overviewRole } from "@/routes/cluster/forbidden"
import { NodesSection, nodeState, share, TIGHT } from "@/routes/cluster/nodes"
import { Section } from "@/routes/cluster/section"
import { useDate } from "@/routes/cluster/upgrade"

export function ClusterPage() {
  return (
    <ClusterFrame tab="overview">
      <Attention />
      <ClusterGaps />
      <NodesSection />
      <WarningsSection />
    </ClusterFrame>
  )
}

/** What the cluster itself is missing (an IngressClass of a retired controller); hidden when
 * nothing is. */
function ClusterGaps() {
  const t = useT()
  const gaps = useQuery(clusterGapsQuery)
  const items = gaps.data?.items ?? []
  if (items.length === 0) return null
  return (
    <Section id="cluster-gaps" title={t("gap.title")} count={items.length}>
      <GapList gaps={items} />
    </Section>
  )
}

// ── Needs attention ──────────────────────────────────────────────────────────────────────────────

interface Item {
  key: string
  health: Health
  text: string
  link?: ReactNode
}

function useAttention(): Item[] {
  const t = useT()
  const date = useDate()
  const { percent } = useQuantityFormat()
  const upgrades = useQuery(upgradesQuery).data
  const nodes = useQuery(nodesQuery).data
  const capabilities = useQuery(capabilitiesQuery).data
  const items: Item[] = []
  const minor = upgrades?.minor ?? ""
  const eol = upgrades?.release?.endOfLife
  if (upgrades?.states.includes("end-of-life"))
    items.push({
      key: "eol",
      health: "fail",
      text: eol ? t("cluster.attEolPast", minor, date(eol)) : t("cluster.attEolOld", minor),
    })
  else if (upgrades?.states.includes("maintenance") && eol)
    items.push({ key: "eol", health: "warn", text: t("cluster.attEolSoon", minor, date(eol)) })
  for (const skew of upgrades?.skew ?? []) {
    if (skew.unsupported)
      items.push({
        key: `skew-${skew.node}`,
        health: "fail",
        text: t("cluster.attSkew", skew.node, skew.kubeletVersion),
      })
  }
  const nodesLink = (
    <a href="#cluster-nodes" className="hover:text-foreground hover:underline">
      {t("cluster.nodes")}
    </a>
  )
  for (const node of nodes?.items ?? []) {
    const state = nodeState(node, t)
    if (state.health !== "ok")
      items.push({
        key: `node-${node.name}`,
        health: state.health,
        text: t("cluster.attNode", node.name, state.word),
        link: nodesLink,
      })
    for (const [what, pick] of [
      [t("cluster.memory"), (r: Schemas["Resources"]) => r.memoryBytes],
      [t("cluster.cpu"), (r: Schemas["Resources"]) => r.cpuMillicores],
    ] as const) {
      const ratio = node.requests ? share(pick(node.requests), pick(node.allocatable)) : undefined
      if (ratio !== undefined && ratio >= TIGHT)
        items.push({
          key: `tight-${node.name}-${what}`,
          health: "warn",
          text: t("cluster.attTight", node.name, what, percent(ratio)),
          link: nodesLink,
        })
    }
  }
  for (const installation of capabilities?.items.flatMap((c) => c.installations) ?? []) {
    if (installation.upgrade === "behind-major" || installation.upgrade === "behind-minor")
      items.push({
        key: `component-${installation.component}`,
        health: "progress",
        text: t(
          "cluster.attComponent",
          installation.displayName,
          installation.version ?? "",
          installation.tested ?? "",
        ),
        link: (
          <Link to="/cluster/components" className="hover:text-foreground hover:underline">
            {t("settings.components")}
          </Link>
        ),
      })
  }
  return items.toSorted((a, b) => healthRank[a.health] - healthRank[b.health])
}

function Attention() {
  const t = useT()
  const items = useAttention()
  const [all, setAll] = useState(false)
  if (items.length === 0)
    return (
      <p className="flex items-center gap-2 text-muted-foreground">
        <StatusDot health="ok" />
        {t("cluster.allClear")}
      </p>
    )
  const shown = all ? items : items.slice(0, 5)
  return (
    <section aria-labelledby="cluster-attention-title">
      <Card size="sm">
        <CardHeader>
          <CardTitle>
            <span className="flex items-baseline gap-2.5">
              <h2 id="cluster-attention-title" className="type-heading">
                {t("attention.title")}
              </h2>
              <span className="type-meta font-normal text-muted-foreground">{items.length}</span>
            </span>
          </CardTitle>
        </CardHeader>
        <CardContent>
          <ul className="flex flex-col">
            {shown.map((item) => (
              <li
                key={item.key}
                className="flex flex-wrap items-center gap-x-3 gap-y-1 border-t border-rule py-2 first:border-t-0"
              >
                <StatusDot health={item.health} />
                <span className="min-w-0 flex-1">{item.text}</span>
                {item.link ? (
                  <span className="type-meta text-muted-foreground">{item.link}</span>
                ) : null}
              </li>
            ))}
          </ul>
          {items.length > 5 ? (
            <Button variant="ghost" size="sm" onClick={() => setAll(!all)} aria-expanded={all}>
              <CaretDownIcon aria-hidden="true" data-icon="inline-start" />
              {all ? t("cluster.fewer") : t("cluster.more", items.length - 5)}
            </Button>
          ) : null}
        </CardContent>
      </Card>
    </section>
  )
}

// ── Warnings ─────────────────────────────────────────────────────────────────────────────────────

const WARNINGS_SHOWN = 20

function WarningsSection() {
  const t = useT()
  const warnings = useQuery(clusterWarningsQuery)
  const data = warnings.data
  const total = data?.totalSize ?? 0
  return (
    <Section id="cluster-warnings" title={t("cluster.warnings")} count={data ? total : undefined}>
      {warnings.isPending ? (
        <LoadingRows label={t("common.loading")} rows={2} />
      ) : warnings.isError ? (
        isStatus(warnings.error, 403) ? (
          <Forbidden
            error={warnings.error}
            what={t("cluster.whatWarnings")}
            missing={[{ group: "events.k8s.io", resources: ["events"] }]}
            role={overviewRole}
          />
        ) : (
          <Problem error={warnings.error} />
        )
      ) : data && data.items.length === 0 ? (
        <p className="flex items-center gap-2 text-muted-foreground">
          <StatusDot health="ok" />
          {t("cluster.noWarnings")}
        </p>
      ) : data ? (
        <>
          <ul className="flex flex-col">
            {data.items.slice(0, WARNINGS_SHOWN).map((event) => (
              <WarningRow key={event.id} event={event} />
            ))}
          </ul>
          {total > WARNINGS_SHOWN || data.partial ? (
            <p className="type-meta text-muted-foreground">
              {[
                total > WARNINGS_SHOWN ? t("cluster.moreWarnings", total - WARNINGS_SHOWN) : "",
                data.partial ? t("cluster.partialEvents") : "",
              ]
                .filter(Boolean)
                .join(t("common.sep"))}
            </p>
          ) : null}
        </>
      ) : null}
    </Section>
  )
}

function WarningRow({ event }: { event: Schemas["EventView"] }) {
  const t = useT()
  const namespace = event.origin.namespace ?? event.regarding?.namespace
  const object = event.regarding ? `${event.regarding.kind}/${event.regarding.name}` : ""
  return (
    <li className="flex flex-wrap items-baseline gap-x-3 gap-y-0.5 border-t border-rule py-2 first:border-t-0">
      <StatusDot health="warn" className="self-center" />
      <span className="font-medium" translate="no">
        {event.reason}
      </span>
      {event.count > 1 ? (
        <span className="type-meta text-muted-foreground">×{event.count}</span>
      ) : null}
      <span className="min-w-0 flex-1 truncate type-meta text-muted-foreground" title={event.note}>
        {namespace ? (
          <Link
            to="/projects/$project"
            params={{ project: namespace }}
            search={{ events: "all" }}
            className="type-mono text-foreground hover:underline"
            translate="no"
          >
            {namespace}
          </Link>
        ) : null}
        {namespace && object ? " " : null}
        <span className="type-mono" translate="no">
          {object}
        </span>
        {event.note ? `${t("common.sep")}${event.note}` : null}
      </span>
      <Ago iso={event.lastSeen} className="type-meta text-muted-foreground" />
    </li>
  )
}
