import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { useState } from "react"
import CaretDownIcon from "~icons/ph/caret-down"

import { isStatus, isUnreachable, type Schemas } from "@/api/client"
import { clusterAlertsQuery, clusterGapsQuery, clusterWarningsQuery } from "@/api/queries"
import { AlertList, RecentNote } from "@/components/alerts"
import { folded } from "@/model/alerts"
import { Ago } from "@/components/bits"
import { GapList } from "@/components/gaps"
import { LoadingRows } from "@/components/page"
import { Problem } from "@/components/problem"
import { StatusDot } from "@/components/status"
import { NotRead } from "@/components/unreachable"
import { Button } from "@zeabur/design-system/components/button"
import { Card, CardContent, CardHeader, CardTitle } from "@zeabur/design-system/components/card"
import { useT } from "@/i18n"
import { ClusterFrame } from "@/routes/cluster/frame"
import { Forbidden, overviewRole } from "@/routes/cluster/forbidden"
import { AttentionRow, useAttention } from "@/routes/cluster/attention"
import { DomainsSection } from "@/routes/cluster/domains"
import { HealthSection } from "@/routes/cluster/health"
import { IssuersSection } from "@/routes/cluster/issuers"
import { NodesSection } from "@/routes/cluster/nodes"
import { UsageSection } from "@/routes/cluster/usage"
import { Section } from "@/routes/cluster/section"

export function ClusterPage() {
  return (
    <ClusterFrame tab="overview">
      <Attention />
      <ClusterGaps />
      <HealthSection />
      <NodesSection />
      <UsageSection />
      <WarningsSection />
      <DomainsSection />
      <IssuersSection />
      <NodeAlerts />
    </ClusterFrame>
  )
}

const ALERTS_SHOWN = 20

/** Falco's alerts about no namespace: a node's own processes. Shown only when there are some. */
function NodeAlerts() {
  const t = useT()
  const data = useQuery(clusterAlertsQuery).data
  if (data?.state !== "installed" || data.items.length === 0) return null
  return (
    <Section id="cluster-alerts" title={t("alerts.nodes")} count={folded(data.items).length}>
      <div className="flex flex-col gap-2">
        <AlertList alerts={data.items} limit={ALERTS_SHOWN} />
        <RecentNote />
      </div>
    </Section>
  )
}

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

const ATTENTION_SHOWN = 5

function Attention() {
  const t = useT()
  const { items, pending, checked, unreachable } = useAttention()
  const [all, setAll] = useState(false)
  if (pending) return null
  if (items.length === 0)
    return (
      <p className="flex items-center gap-2 text-muted-foreground">
        <StatusDot health={checked ? "ok" : "idle"} />
        {checked
          ? t("cluster.allClear")
          : unreachable
            ? t("cluster.cannotCheck")
            : t("cluster.notChecked")}
      </p>
    )
  const shown = all ? items : items.slice(0, ATTENTION_SHOWN)
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
              <AttentionRow key={item.key} item={item} />
            ))}
          </ul>
          {items.length > ATTENTION_SHOWN ? (
            <Button variant="ghost" size="sm" onClick={() => setAll(!all)} aria-expanded={all}>
              <CaretDownIcon aria-hidden="true" data-icon="inline-start" />
              {all ? t("cluster.fewer") : t("cluster.more", items.length - ATTENTION_SHOWN)}
            </Button>
          ) : null}
          {checked ? null : (
            <p className="type-meta text-muted-foreground">{t("cluster.notChecked")}</p>
          )}
        </CardContent>
      </Card>
    </section>
  )
}

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
        isUnreachable(warnings.error) ? (
          <NotRead />
        ) : isStatus(warnings.error, 403) ? (
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

function WarningRow({ event }: { event: Schemas.EventView }) {
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
      <span
        className="w-full min-w-0 type-meta break-words text-muted-foreground sm:w-auto sm:flex-1 sm:truncate"
        title={event.note}
      >
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
