// What in the cluster needs someone: end of support, version skew, nodes down, full or under
// pressure, components behind. Each item says what happens because of it and what to do; the same
// outcome on many nodes is one item. The cluster page lists them, and Home lifts the ones that take
// services down.
import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import type { ReactNode } from "react"
import CaretRightIcon from "~icons/ph/caret-right"

import { isStatus, isUnreachable } from "@/api/client"
import { capabilitiesQuery, nodesQuery, upgradesQuery } from "@/api/queries"
import { StatusDot } from "@/components/status"
import { useT } from "@/i18n"
import { useQuantityFormat } from "@/lib/quantity"
import { groupIssues, isProblem, nodeIssues, type IssueGroup } from "@/model/node-health"
import { healthRank, type Health } from "@/model/service"
import { useDate } from "@/routes/cluster/upgrade"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur/design-system/components/collapsible"

export interface ClusterItem {
  key: string
  health: Health
  text: string
  /** What follows from it and what to do. */
  detail?: string
  /** The nodes or components it covers when there are several, behind a disclosure. */
  members?: { key: string; label: string; note?: string }[]
  link?: ReactNode
  /** The nodes it is about. */
  nodes: string[]
  /** It takes services down, or is about to; else it only limits what comes next. */
  problem: boolean
}

export interface Attention {
  items: ClusterItem[]
  pending: boolean
  /** Every read the list comes from answered (the components' refusal counts as an answer). */
  checked: boolean
  /** No read got an answer because the cluster does not answer. */
  unreachable: boolean
}

/** A node issue group's sentence: what, then what follows and the next step. */
export function useIssueText() {
  const t = useT()
  const { percent } = useQuantityFormat()
  return (group: IssueGroup): { text: string; detail: string } => {
    const first = group.issues[0]
    const n = group.issues.length
    const node = first?.node ?? ""
    const share = first?.share !== undefined ? percent(first.share) : ""
    const severe = group.health === "fail"
    switch (group.kind) {
      case "memoryFull":
        return {
          text: t("node.memoryFull", n, node, share),
          detail: `${severe ? t("node.memoryFull.fail") : t("node.memoryFull.warn")} ${t("node.memoryFull.fix")}`,
        }
      case "diskFull":
        return {
          text: t("node.diskFull", n, node, share),
          detail: `${severe ? t("node.diskFull.fail") : t("node.diskFull.warn")} ${t("node.diskFull.fix")}`,
        }
      case "cordoned":
        return {
          text: t("node.cordoned", n, node),
          detail: `${t("node.cordoned.why")} ${t("node.cordoned.fix", n, node)}`,
        }
      case "reservedMemory":
      case "reservedCpu":
        return {
          text: t(`node.${group.kind}`, n, node, share),
          detail: `${t(`node.${group.kind}.why`)} ${t(`node.${group.kind}.fix`)}`,
        }
      default:
        return {
          text: t(`node.${group.kind}`, n, node),
          detail: `${t(`node.${group.kind}.why`)} ${t(`node.${group.kind}.fix`)}`,
        }
    }
  }
}

export function useAttention(): Attention {
  const t = useT()
  const date = useDate()
  const { percent } = useQuantityFormat()
  const issueText = useIssueText()
  const upgradesRead = useQuery(upgradesQuery)
  const nodesRead = useQuery(nodesQuery)
  const capabilitiesRead = useQuery(capabilitiesQuery)
  const upgrades = upgradesRead.data
  const nodes = nodesRead.data
  const capabilities = capabilitiesRead.data
  const items: ClusterItem[] = []
  const minor = upgrades?.minor ?? ""
  const eol = upgrades?.release?.endOfLife
  if (upgrades?.states.includes("end-of-life"))
    items.push({
      key: "eol",
      health: "fail",
      text: eol ? t("cluster.attEolPast", minor, date(eol)) : t("cluster.attEolOld", minor),
      detail: `${t("cluster.attEol.why")} ${t("cluster.attEol.fix")}`,
      nodes: [],
      problem: false,
    })
  else if (upgrades?.states.includes("maintenance") && eol)
    items.push({
      key: "eol",
      health: "warn",
      text: t("cluster.attEolSoon", minor, date(eol)),
      detail: `${t("cluster.attEolSoon.why")} ${t("cluster.attEol.fix")}`,
      nodes: [],
      problem: false,
    })
  const skewed = (upgrades?.skew ?? []).filter((s) => s.unsupported)
  if (skewed.length > 0)
    items.push({
      key: "skew",
      health: "fail",
      text:
        skewed.length === 1
          ? t("cluster.attSkew", skewed[0]?.node ?? "", skewed[0]?.kubeletVersion ?? "")
          : t("cluster.attSkewMany", skewed.length),
      detail: `${t("cluster.attSkew.why")} ${t("cluster.attSkew.fix")}`,
      members:
        skewed.length > 1
          ? skewed.map((s) => ({ key: s.node, label: s.node, note: s.kubeletVersion }))
          : undefined,
      nodes: skewed.map((s) => s.node),
      problem: false,
    })
  const nodesLink = (
    <Link to="/cluster" hash="cluster-nodes" className="hover:text-foreground hover:underline">
      {t("cluster.nodes")}
    </Link>
  )
  for (const group of groupIssues((nodes?.items ?? []).flatMap(nodeIssues))) {
    const { text, detail } = issueText(group)
    items.push({
      key: `node-${group.kind}`,
      health: group.health,
      text,
      detail,
      members:
        group.issues.length > 1
          ? group.issues.map((i) => ({
              key: i.node,
              label: i.node,
              note: i.share !== undefined ? percent(i.share) : undefined,
            }))
          : undefined,
      link: nodesLink,
      nodes: group.issues.map((i) => i.node),
      problem: group.issues.some(isProblem),
    })
  }
  const behind = (capabilities?.items.flatMap((c) => c.installations) ?? []).filter(
    (i) => i.upgrade === "behind-major" || i.upgrade === "behind-minor",
  )
  if (behind.length > 0)
    items.push({
      key: "components-behind",
      health: "warn",
      text:
        behind.length === 1
          ? t(
              "cluster.attComponent",
              behind[0]?.displayName ?? "",
              behind[0]?.version ?? "",
              behind[0]?.tested ?? "",
            )
          : t("cluster.attComponents", behind.length),
      detail: `${t("cluster.attComponent.why")} ${t("cluster.attComponent.fix")}`,
      members:
        behind.length > 1
          ? behind.map((i) => ({
              key: i.component,
              label: i.displayName,
              note: t("cluster.versionOf", i.version ?? "", i.tested ?? ""),
            }))
          : undefined,
      link: (
        <Link to="/cluster/components" className="hover:text-foreground hover:underline">
          {t("settings.components")}
        </Link>
      ),
      nodes: [],
      problem: false,
    })
  const answered = (read: { isSuccess: boolean }) => read.isSuccess
  const reads = [upgradesRead, nodesRead, capabilitiesRead]
  return {
    items: items.toSorted((a, b) => healthRank[a.health] - healthRank[b.health]),
    pending: reads.some((r) => r.isPending),
    checked:
      answered(upgradesRead) &&
      answered(nodesRead) &&
      (answered(capabilitiesRead) || isStatus(capabilitiesRead.error, 403)),
    unreachable: reads.every((r) => r.isError && isUnreachable(r.error)),
  }
}

/** One cluster item: its dot, what, what follows and the fix, and the nodes behind a
 * disclosure when there are several. */
export function AttentionRow({ item, inset = false }: { item: ClusterItem; inset?: boolean }) {
  const t = useT()
  return (
    <li
      className={
        inset
          ? "flex items-start gap-x-3 border-t border-rule px-4 py-3 first:border-t-0"
          : "flex items-start gap-x-3 border-t border-rule py-2.5 first:border-t-0"
      }
    >
      <StatusDot health={item.health} className="mt-1.5 shrink-0" />
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <div className="flex flex-wrap items-baseline justify-between gap-x-3">
          <span className="min-w-0 font-medium break-words">{item.text}</span>
          {item.link ? (
            <span className="shrink-0 type-meta text-muted-foreground">{item.link}</span>
          ) : null}
        </div>
        {item.detail ? (
          <span className="type-meta text-muted-foreground">{item.detail}</span>
        ) : null}
        {item.members ? (
          <Collapsible>
            <CollapsibleTrigger variant="disclosure">
              <CaretRightIcon aria-hidden="true" />
              {t("cluster.showMembers", item.members.length)}
            </CollapsibleTrigger>
            <CollapsibleContent>
              <ul className="mt-1 flex max-h-60 flex-col overflow-y-auto type-meta">
                {item.members.map((member) => (
                  <li key={member.key} className="flex min-w-0 items-baseline gap-2 py-0.5">
                    <span className="min-w-0 truncate" translate="no" title={member.label}>
                      {member.label}
                    </span>
                    {member.note ? (
                      <span className="shrink-0 text-muted-foreground">{member.note}</span>
                    ) : null}
                  </li>
                ))}
              </ul>
            </CollapsibleContent>
          </Collapsible>
        ) : null}
      </div>
    </li>
  )
}
