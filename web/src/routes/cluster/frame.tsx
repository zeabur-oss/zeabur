// The frame of the cluster's pages: its version and what it calls for, then the tabs Overview
// (/cluster) and Components (/cluster/components). The URL is the tab.
import { useQuery } from "@tanstack/react-query"
import { useNavigate } from "@tanstack/react-router"
import type { ReactNode } from "react"

import { clusterQuery, upgradesQuery } from "@/api/queries"
import { StatusPill } from "@/components/status"
import { Tabs, TabsList, TabsTrigger } from "@zeabur/design-system/components/tabs"
import { useT } from "@/i18n"
import { Forbidden, overviewRole } from "@/routes/cluster/forbidden"
import { useUpgradeBadge } from "@/routes/cluster/upgrade"
import { PageHeader } from "@zeabur/design-system/components/page-header"

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
      className="flex w-full max-w-7xl flex-col"
    >
      <header className="flex flex-col gap-4 px-4 pt-6 md:px-8 md:pt-8">
        <Heading />
        <TabsList aria-label={t("cluster.tabs")}>
          <TabsTrigger value="overview">{t("cluster.overview")}</TabsTrigger>
          <TabsTrigger value="components">{t("settings.components")}</TabsTrigger>
        </TabsList>
      </header>
      <div className="flex flex-col gap-9 px-4 pt-6 pb-12 type-body md:px-8">{children}</div>
    </Tabs>
  )
}

function Heading() {
  const t = useT()
  const cluster = useQuery(clusterQuery)
  const upgrades = useQuery(upgradesQuery)
  const badge = useUpgradeBadge(upgrades.data)
  const data = upgrades.data
  // Plans refused: system-upgrade-controller is installed but its Plans are not readable, so
  // newer versions are not known; say which permission is missing.
  const plansRefused = data?.unreadable?.some((u) => u.resource === "plans.upgrade.cattle.io")
  const version = cluster.data?.kubernetesVersion
  return (
    <PageHeader
      title={t("cluster.title")}
      badges={
        <>
          {version ? (
            // The version usually names the distribution (`v1.36.5+k3s1`); otherwise it goes first.
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
