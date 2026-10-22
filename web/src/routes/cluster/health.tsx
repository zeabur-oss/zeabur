// The API server's readiness checks (/readyz) and the scheduler's and controller manager's
// leader leases: one line when all is well, the failing checks named otherwise.
import { useQuery } from "@tanstack/react-query"
import CaretRightIcon from "~icons/ph/caret-right"

import { isStatus, isUnreachable, toProblem, type Schemas } from "@/api/client"
import { clusterHealthQuery } from "@/api/queries"
import { Ago } from "@/components/bits"
import { LoadingRows } from "@/components/page"
import { Problem } from "@/components/problem"
import { Refused } from "@/components/refused"
import { StatusDot } from "@/components/status"
import { NotRead } from "@/components/unreachable"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur/design-system/components/collapsible"
import { useT } from "@/i18n"
import { Section } from "@/routes/cluster/section"

type ClusterHealth = Schemas["ClusterHealth"]

export function HealthSection() {
  const t = useT()
  const health = useQuery(clusterHealthQuery)
  return (
    <Section id="cluster-api-server" title={t("cluster.apiServer")}>
      {health.isPending ? (
        <LoadingRows label={t("common.loading")} rows={1} />
      ) : health.isError ? (
        isUnreachable(health.error) ? (
          <NotRead />
        ) : isStatus(health.error, 401) || isStatus(health.error, 403) ? (
          <Refused
            refusal={toProblem(health.error)}
            what={t("cluster.whatApiServer")}
            missing={["get /readyz"]}
          />
        ) : (
          <div className="flex flex-col gap-2">
            <p className="flex items-center gap-2 text-muted-foreground">
              <StatusDot health="idle" />
              {t("cluster.apiUnknown")}
            </p>
            <Problem error={health.error} />
          </div>
        )
      ) : (
        <Readiness health={health.data} />
      )}
    </Section>
  )
}

function Readiness({ health }: { health: ClusterHealth }) {
  const t = useT()
  const stale = health.leaders.filter((lease) => !lease.current)
  const failing = health.failing.length
  return (
    <div className="flex flex-col gap-2">
      <p className="flex items-center gap-2">
        <StatusDot health={failing > 0 ? "fail" : stale.length > 0 ? "warn" : "ok"} />
        <span className={failing > 0 ? "font-medium" : undefined}>
          {failing > 0 ? t("cluster.apiFailing", failing) : t("cluster.apiReady")}
        </span>
      </p>
      {failing > 0 || stale.length > 0 ? (
        <ul className="flex flex-col">
          {health.failing.map((check) => (
            <li
              key={check.name}
              className="flex flex-wrap items-baseline gap-x-3 gap-y-0.5 border-t border-rule py-2 first:border-t-0"
            >
              <span className="type-mono" translate="no">
                {check.name}
              </span>
              <span className="type-meta text-muted-foreground" lang="en">
                {check.reason ?? t("cluster.apiCheckFailed")}
              </span>
            </li>
          ))}
          {stale.map((lease) => (
            <li
              key={lease.component}
              className="flex flex-wrap items-baseline gap-x-3 gap-y-0.5 border-t border-rule py-2 first:border-t-0"
            >
              <span className="type-mono" translate="no">
                {lease.component}
              </span>
              <span className="type-meta text-muted-foreground">
                {t("cluster.leaseStale")} <Ago iso={lease.renewed} />
              </span>
            </li>
          ))}
        </ul>
      ) : null}
      <Coverage health={health} />
    </div>
  )
}

function Coverage({ health }: { health: ClusterHealth }) {
  const t = useT()
  return (
    <Collapsible>
      <CollapsibleTrigger variant="disclosure">
        <CaretRightIcon aria-hidden="true" />
        {t("refused.details")}
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="flex flex-col gap-2 pt-2 type-meta text-muted-foreground">
          <p>{t("cluster.apiCovers", health.checks)}</p>
          <p>{health.leaders.length > 0 ? t("cluster.apiLeases") : t("cluster.apiNotCovered")}</p>
          {health.leaders.length > 0 ? (
            <dl className="grid grid-cols-pairs gap-x-4 gap-y-1">
              {health.leaders.map((lease) => (
                <div key={lease.component} className="contents">
                  <dt className="type-mono" translate="no">
                    {lease.component}
                  </dt>
                  <dd>
                    {lease.current ? t("cluster.leaseRenewed") : t("cluster.leaseStale")}{" "}
                    <Ago iso={lease.renewed} />
                  </dd>
                </div>
              ))}
            </dl>
          ) : null}
        </div>
      </CollapsibleContent>
    </Collapsible>
  )
}
