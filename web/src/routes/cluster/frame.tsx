import { useQuery } from "@tanstack/react-query"
import { useNavigate } from "@tanstack/react-router"
import type { ReactNode } from "react"

import { clusterQuery, upgradesQuery } from "@/api/queries"
import { StatusPill } from "@/components/status"
import { UnreachableBanner, useReachability } from "@/components/unreachable"
import { Tabs, TabsList, TabsTrigger } from "@zeabur/design-system/components/tabs"
import { useT } from "@/i18n"
import { Forbidden, overviewRole } from "@/routes/cluster/forbidden"
import { useUpgradeBadge } from "@/routes/cluster/upgrade"
import { PageHeader } from "@zeabur/design-system/components/page-header"
import { PageColumn } from "@zeabur/design-system/components/page-column"

const paths = { overview: "/cluster", components: "/cluster/components" } as const

export type ClusterTab = keyof typeof paths

export function ClusterFrame({ tab, children }: { tab: ClusterTab; children: ReactNode }) {
  const t = useT()
  const navigate = useNavigate()
  return (
    <Tabs
      value={tab}
      onValueChange={(value) => void navigate({ to: paths[value as ClusterTab] })}
      variant="page"
      className="flex flex-col"
    >
      <header className="pt-6 md:pt-8">
        <PageColumn gap="sm">
          <Heading />
          <TabsList aria-label={t("cluster.tabs")}>
            <TabsTrigger value="overview">{t("cluster.overview")}</TabsTrigger>
            <TabsTrigger value="components">{t("settings.components")}</TabsTrigger>
          </TabsList>
        </PageColumn>
      </header>
      <PageColumn gap="lg" className="mt-6 mb-12">
        <UnreachableBanner />
        {children}
      </PageColumn>
    </Tabs>
  )
}

function Heading() {
  const t = useT()
  const cluster = useQuery(clusterQuery)
  const upgrades = useQuery(upgradesQuery)
  const badge = useUpgradeBadge(upgrades.data)
  const data = upgrades.data
  // Without readable Plans newer versions are unknown.
  const plansRefused = data?.unreadable?.some((u) => u.resource === "plans.upgrade.cattle.io")
  const name = useReachability().name
  const version = cluster.data?.kubernetesVersion
  return (
    <PageHeader
      title={t("cluster.title")}
      badges={
        <>
          {name ? (
            <span
              className={
                cluster.data ? "font-medium text-main" : "font-medium text-muted-foreground"
              }
              translate="no"
            >
              {name}
            </span>
          ) : null}
          {version ? (
            <span className="type-mono text-muted-foreground" translate="no">
              {cluster.data?.distribution && !version.includes(cluster.data.distribution)
                ? `${cluster.data.distribution} ${version}`
                : version}
            </span>
          ) : null}
          {badge ? <StatusPill health={badge.health}>{badge.text}</StatusPill> : null}
        </>
      }
      description={
        data && (data.patch || data.nextMinor)
          ? [
              data.patch ? t("cluster.patch", data.patch) : "",
              data.nextMinor ? t("cluster.nextMinor", data.nextMinor) : "",
            ]
              .filter(Boolean)
              .join(t("common.sep"))
          : undefined
      }
    >
      {plansRefused ? (
        <Forbidden
          what={t("cluster.whatUpgrades")}
          missing={[{ group: "upgrade.cattle.io", resources: ["plans"] }]}
          role={overviewRole}
        />
      ) : null}
    </PageHeader>
  )
}
