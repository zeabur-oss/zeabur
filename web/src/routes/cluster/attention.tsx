// What in the cluster needs someone: end of support, version skew, nodes down or full, components
// behind. The cluster page lists it, and Home puts it above the services it takes down.
import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import type { ReactNode } from "react"

import type { Schemas } from "@/api/client"
import { capabilitiesQuery, nodesQuery, upgradesQuery } from "@/api/queries"
import { useT } from "@/i18n"
import { useQuantityFormat } from "@/lib/quantity"
import { healthRank, type Health } from "@/model/service"
import { nodeState, share, TIGHT } from "@/routes/cluster/nodes"
import { useDate } from "@/routes/cluster/upgrade"

export interface ClusterItem {
  key: string
  health: Health
  text: string
  link?: ReactNode
  /** The node it is about, whose pods it reaches. */
  node?: string
}

export function useAttention(): ClusterItem[] {
  const t = useT()
  const date = useDate()
  const { percent } = useQuantityFormat()
  const upgrades = useQuery(upgradesQuery).data
  const nodes = useQuery(nodesQuery).data
  const capabilities = useQuery(capabilitiesQuery).data
  const items: ClusterItem[] = []
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
    <Link to="/cluster" hash="cluster-nodes" className="hover:text-foreground hover:underline">
      {t("cluster.nodes")}
    </Link>
  )
  for (const node of nodes?.items ?? []) {
    const state = nodeState(node, t)
    if (state.health !== "ok")
      items.push({
        key: `node-${node.name}`,
        health: state.health,
        text: t("cluster.attNode", node.name, state.word),
        link: nodesLink,
        node: node.name,
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
