import { useQuery } from "@tanstack/react-query"
import { getRouteApi, Link, useNavigate } from "@tanstack/react-router"
import { lazy, Suspense, useState } from "react"
import ArchiveIcon from "~icons/ph/archive"
import ArrowSquareOutIcon from "~icons/ph/arrow-square-out"
import ArrowsClockwiseIcon from "~icons/ph/arrows-clockwise"
import CertificateIcon from "~icons/ph/certificate"
import ChartLineIcon from "~icons/ph/chart-line"
import ClockCounterClockwiseIcon from "~icons/ph/clock-counter-clockwise"
import CloudArrowUpIcon from "~icons/ph/cloud-arrow-up"
import DatabaseIcon from "~icons/ph/database"
import DownloadSimpleIcon from "~icons/ph/download-simple"
import GaugeIcon from "~icons/ph/gauge"
import GlobeIcon from "~icons/ph/globe"
import HardDriveIcon from "~icons/ph/hard-drive"
import HardDrivesIcon from "~icons/ph/hard-drives"
import HeartbeatIcon from "~icons/ph/heartbeat"
import LightningIcon from "~icons/ph/lightning"
import MagnifyingGlassIcon from "~icons/ph/magnifying-glass"
import PackageIcon from "~icons/ph/package"
import PulseIcon from "~icons/ph/pulse"
import ShieldCheckIcon from "~icons/ph/shield-check"
import SignpostIcon from "~icons/ph/signpost"
import SlidersHorizontalIcon from "~icons/ph/sliders-horizontal"
import TextAlignLeftIcon from "~icons/ph/text-align-left"

import { isStatus, type Schemas } from "@/api/client"
import { capabilitiesQuery, catalogQuery, clusterIncidentsQuery } from "@/api/queries"
import { useCapabilityLabel } from "@/components/capabilities"
import { Segmented } from "@/components/bits"
import { LoadingRows } from "@/components/page"
import { Problem } from "@/components/problem"
import { StatusPill } from "@/components/status"
import { Button, buttonVariants } from "@zeabur-oss/design-system/components/button"
import { Skeleton } from "@zeabur-oss/design-system/components/skeleton"
import { Alert, AlertDescription, AlertTitle } from "@zeabur-oss/design-system/components/alert"
import { Tooltip } from "@zeabur-oss/design-system/components/tooltip"
import { type Locale, useI18n, useT } from "@/i18n"
import type { Icon } from "@/lib/icon"
import { docsUrl } from "@/lib/docs"
import { formatAgo } from "@/lib/format"
import { cn } from "@/lib/utils"
import { componentsRole, Forbidden } from "@/routes/cluster/forbidden"
import { ClusterFrame } from "@/routes/cluster/frame"
import { Section } from "@/routes/cluster/section"

const route = getRouteApi("/cluster/components")

const ComponentPlan = lazy(() =>
  import("@/components/component-plan").then((m) => ({
    default: m.ComponentPlan,
  })),
)

const categories: {
  key: "traffic" | "observability" | "scaling" | "security" | "data" | "operations"
  names: string[]
}[] = [
  { key: "traffic", names: ["ingress", "certificates", "dns"] },
  {
    key: "observability",
    names: ["metrics", "logs", "observability-operator", "uptime-probes"],
  },
  { key: "scaling", names: ["event-autoscaling", "resource-recommendations"] },
  {
    key: "security",
    names: ["intrusion-prevention", "runtime-security", "vulnerability-scanning"],
  },
  {
    key: "data",
    names: ["databases", "postgres", "object-storage", "replicated-storage"],
  },
  {
    key: "operations",
    names: ["node-updates", "volume-backup", "builds", "image-updates"],
  },
]

const icons: Record<string, Icon> = {
  ingress: SignpostIcon,
  certificates: CertificateIcon,
  dns: GlobeIcon,
  metrics: ChartLineIcon,
  logs: TextAlignLeftIcon,
  "observability-operator": PulseIcon,
  "uptime-probes": HeartbeatIcon,
  "event-autoscaling": LightningIcon,
  "resource-recommendations": GaugeIcon,
  "intrusion-prevention": ShieldCheckIcon,
  "runtime-security": ShieldCheckIcon,
  "vulnerability-scanning": MagnifyingGlassIcon,
  "node-updates": CloudArrowUpIcon,
  "volume-backup": ClockCounterClockwiseIcon,
  databases: DatabaseIcon,
  postgres: DatabaseIcon,
  "replicated-storage": HardDriveIcon,
  "object-storage": ArchiveIcon,
  builds: PackageIcon,
  "image-updates": ArrowsClockwiseIcon,
}

type Filter = "all" | "installed" | "recommended"

export function ComponentsPage() {
  const { t, locale } = useI18n()
  const capLabel = useCapabilityLabel()
  const capabilities = useQuery(capabilitiesQuery)
  const search = route.useSearch()
  const navigate = useNavigate({ from: "/cluster/components" })
  const [filter, setFilter] = useState<Filter>("all")
  const [query, setQuery] = useState("")

  if (capabilities.isError && isStatus(capabilities.error, 403))
    return (
      <ClusterFrame tab="components">
        <Forbidden
          error={capabilities.error}
          what={t("cluster.whatComponents")}
          missing={componentsRole.rules}
          role={componentsRole}
        />
      </ClusterFrame>
    )

  const list = capabilities.data?.items ?? []
  const installed = list.filter((c) => c.existing).length
  const recommended = list.filter((c) => !c.existing && c.suggestion === "recommended").length
  const keep = (c: Schemas.Capability) =>
    (filter === "all" ||
      (filter === "installed" && c.existing) ||
      (filter === "recommended" && !c.existing && c.suggestion === "recommended")) &&
    (!query ||
      `${c.name} ${capLabel(c.name)} ${c.components.map((x) => `${x.displayName} ${summaryOf(x, locale)}`).join(" ")}`
        .toLowerCase()
        .includes(query.toLowerCase()))
  const known = new Set(categories.flatMap((c) => c.names))
  const groups = [
    ...categories.map((cat) => ({
      key: cat.key as string,
      items: list.filter((c) => cat.names.includes(c.name)),
    })),
    { key: "other", items: list.filter((c) => !known.has(c.name)) },
  ]
    .map((g) => ({ key: g.key, items: g.items.filter(keep) }))
    .filter((g) => g.items.length > 0)

  return (
    <ClusterFrame tab="components">
      {search.component && capabilities.data ? (
        <OpenedPlan
          key={search.component}
          id={search.component}
          list={list}
          onClose={() =>
            void navigate({
              search: (previous) => ({ ...previous, component: undefined }),
            })
          }
        />
      ) : null}
      <CatalogStatus />
      <InstalledSection />

      <section aria-labelledby="components-catalogue" className="flex flex-col gap-4">
        <h2 id="components-catalogue" className="type-heading">
          {t("cluster.catalogue")}
        </h2>
        <div className="flex flex-wrap items-center gap-2">
          <Segmented
            label={t("settings.filter")}
            value={filter}
            onChange={setFilter}
            options={(
              [
                ["all", t("common.all"), list.length],
                ["installed", t("settings.installed"), installed],
                ["recommended", t("settings.recommended"), recommended],
              ] as const
            ).map(([value, label, count]) => ({
              value,
              label: (
                <>
                  {label}
                  <span className="font-normal text-muted-foreground tabular-nums">{count}</span>
                </>
              ),
            }))}
          />
          <label className="ml-auto flex h-8 w-full items-center gap-2 rounded-lg border bg-background px-2.5 sm:w-64">
            <MagnifyingGlassIcon className="size-4 text-muted-foreground" aria-hidden="true" />
            <input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              aria-label={t("settings.searchComponents")}
              placeholder={t("settings.searchComponents")}
              className="w-full bg-transparent text-sm outline-none placeholder:text-muted-foreground"
            />
          </label>
        </div>

        {capabilities.isPending ? (
          <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
            {Array.from({ length: 6 }, (_, i) => (
              <Skeleton key={i} variant="card" className="h-44" />
            ))}
          </div>
        ) : capabilities.isError ? (
          <Problem error={capabilities.error} />
        ) : (
          groups.map((group) => (
            <section key={group.key} className="mt-2 flex flex-col gap-3">
              <h2 className="type-meta font-medium text-muted-foreground">
                {t(`category.${group.key as "traffic"}`)}
              </h2>
              <ul className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
                {group.items.flatMap((capability) =>
                  capability.components
                    .filter(
                      (component) =>
                        filter !== "installed" ||
                        installationOf(capability, component) !== undefined,
                    )
                    .map((component) => (
                      <li key={component.id} className="flex">
                        <ComponentCard capability={capability} component={component} />
                      </li>
                    )),
                )}
              </ul>
            </section>
          ))
        )}
      </section>
    </ClusterFrame>
  )
}

/** What the component gives the cluster, in the catalogue's words for `locale`. */
const summaryOf = (component: Schemas.Component, locale: Locale) =>
  component.summary[locale] ?? component.summary.en

/** This component itself, not another of its capability: metrics-server covers "metrics" without
 * VictoriaMetrics being installed. */
const installationOf = (capability: Schemas.Capability, component: Schemas.Component) =>
  capability.installations.find((i) => i.component === component.id)

/** Install when nothing is installed, manage what Zeabur may have installed; nothing for a
 * component K3s, RKE2 or Helm manages. The dialog's plan decides between installing and removing. */
const buttonOf = (installation: Schemas.Installation | undefined) =>
  !installation
    ? "install"
    : installation.source === "zeabur" || installation.source === "other"
      ? "manage"
      : undefined

/** What a component incident names, as the catalogue's capability or a component id. */
const incidentComponents: Record<string, string> = {
  "ingress-controller": "ingress",
  "cert-manager": "certificates",
  "metrics-server": "metrics-server",
}

/** The installed components an incident says are down, by capability or component id. */
function useDown() {
  const incidents = useQuery(clusterIncidentsQuery).data?.items ?? []
  const down = new Set(
    incidents
      .filter((i) => i.kind === "componentDown" && i.fault.component)
      .map((i) => incidentComponents[i.fault.component ?? ""])
      .filter((key): key is string => !!key),
  )
  return (capability: string, component: string) => down.has(capability) || down.has(component)
}

/** The plan of the component in the address. */
function OpenedPlan({
  id,
  list,
  onClose,
}: {
  id: string
  list: Schemas.Capability[]
  onClose: () => void
}) {
  const opened = list.flatMap((c) => c.components).find((x) => x.id === id)
  const installation = list.flatMap((c) => c.installations).find((i) => i.component === id)
  return (
    <Suspense fallback={null}>
      <ComponentPlan
        id={id}
        name={opened?.displayName ?? id}
        installed={buttonOf(installation) === "manage"}
        onClose={onClose}
      />
    </Suspense>
  )
}

/** Which Zeabur Catalog release the page shows, and why Zeabur could not read a newer one. */
function CatalogStatus() {
  const { t, locale } = useI18n()
  const query = useQuery(catalogQuery)
  const catalog = query.data
  if (!catalog) return null
  if (!catalog.version)
    return (
      <Alert variant="warn">
        <AlertTitle>{t("components.catalogUnreachable")}</AlertTitle>
        <AlertDescription>
          <p>{t("components.catalogUnaffected")}</p>
          <p className="font-mono break-all" translate="no">
            {catalog.url}
            {catalog.error ? `: ${catalog.error}` : ""}
          </p>
          <p>{t("components.catalogMirror")}</p>
          <Button
            variant="outline"
            size="xs"
            loading={query.isFetching}
            onClick={() => void query.refetch()}
          >
            {t("common.retry")}
          </Button>
        </AlertDescription>
      </Alert>
    )
  return (
    <p className="type-meta text-muted-foreground">
      {t(
        "components.catalog",
        catalog.version,
        catalog.fetchedAt ? formatAgo(catalog.fetchedAt, locale) : "",
      )}
      {catalog.error ? `${t("common.sep")}${t("components.catalogKept", catalog.error)}` : ""}
      {catalog.cacheError
        ? `${t("common.sep")}${t("components.catalogInMemory", catalog.cacheError)}`
        : ""}
    </p>
  )
}

function ComponentCard({
  capability,
  component,
}: {
  capability: Schemas.Capability
  component: Schemas.Component
}) {
  const { t, locale } = useI18n()
  const capLabel = useCapabilityLabel()
  const CapIcon = icons[capability.name] ?? HardDrivesIcon
  const isDown = useDown()
  const installation = installationOf(capability, component)
  const installed = installation !== undefined
  const down = installed && isDown(capability.name, component.id)
  const state = down ? "down" : installed ? "installed" : (capability.suggestion ?? "optional")
  return (
    <article className="flex w-full flex-col gap-3 rounded-xl border border-v2-border bg-card p-4 transition-colors hover:border-foreground/20">
      <div className="flex items-start gap-3">
        <span className="flex size-6 shrink-0 items-center justify-center text-muted-foreground">
          <CapIcon className="size-5" aria-hidden="true" />
        </span>
        <div className="flex min-w-0 flex-1 flex-col">
          <h3 className="truncate font-semibold">{component.displayName}</h3>
          <span className="text-xs text-muted-foreground">{capLabel(capability.name)}</span>
        </div>
        <StateChip state={state} evidence={capability.evidence} />
      </div>
      <p className="line-clamp-3 type-body leading-normal text-muted-foreground">
        {summaryOf(component, locale)}
      </p>
      {installation?.packagedChart ? (
        <PackagedNote by={t(sourceKey[installation.source])} chart={installation.packagedChart} />
      ) : null}
      <div className="mt-auto flex min-h-8 items-center gap-2 pt-1">
        <span className="type-meta text-muted-foreground" translate="no">
          {component.license}
        </span>
        <a
          href={component.homepage}
          target="_blank"
          rel="noreferrer noopener"
          className="inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground"
        >
          {t("settings.homepage")}
          <ArrowSquareOutIcon className="size-3" aria-hidden="true" />
          <span className="sr-only">{t("settings.opensInNewTab")}</span>
        </a>
        <CardAction id={component.id} button={buttonOf(installation)} />
      </div>
    </article>
  )
}

/** A component K3s or RKE2 deployed as a chart: its values change through a HelmChartConfig. */
function PackagedNote({ by, chart }: { by: string; chart: string }) {
  const { t, locale } = useI18n()
  return (
    <p className="type-meta text-muted-foreground">
      {t("components.packaged", by, chart)}{" "}
      <a
        href={docsUrl(locale, "cluster/components", t("components.packagedAnchor"))}
        target="_blank"
        rel="noreferrer noopener"
        className="inline-flex items-center gap-1 underline underline-offset-2 hover:text-foreground"
      >
        {t("components.packagedHow")}
        <ArrowSquareOutIcon className="size-3" aria-hidden="true" />
        <span className="sr-only">{t("settings.opensInNewTab")}</span>
      </a>
    </p>
  )
}

/** Opens the component's plan. */
function CardAction({ id, button }: { id: string; button: "install" | "manage" | undefined }) {
  const t = useT()
  const catalog = useQuery(catalogQuery).data
  const offline = catalog !== undefined && !catalog.version
  if (!button) return null
  return (
    <Link
      from="/cluster/components"
      search={(previous) => ({
        ...previous,
        component: id,
      })}
      disabled={offline}
      aria-disabled={offline}
      className={cn(
        buttonVariants({ variant: "outline", size: "xs" }),
        "ml-auto aria-disabled:pointer-events-none aria-disabled:opacity-50",
      )}
    >
      {button === "manage" ? (
        <SlidersHorizontalIcon aria-hidden="true" data-icon="inline-start" />
      ) : (
        <DownloadSimpleIcon aria-hidden="true" data-icon="inline-start" />
      )}
      {button === "manage" ? t("components.manage") : t("settings.installCommand")}
    </Link>
  )
}

function StateChip({ state, evidence }: { state: string; evidence: string[] }) {
  const t = useT()
  if (state === "down")
    return (
      <StatusPill health="fail" size="sm" className="shrink-0">
        {t("status.failing")}
      </StatusPill>
    )
  if (state === "installed")
    return (
      <Tooltip
        content={
          <ul className="flex flex-col gap-0.5">
            {evidence.map((line) => (
              <li key={line} className="font-mono">
                {line}
              </li>
            ))}
          </ul>
        }
      >
        <span className="flex shrink-0 cursor-help">
          <StatusPill health="ok" size="sm">
            {t("settings.installed")}
          </StatusPill>
        </span>
      </Tooltip>
    )
  if (state === "recommended")
    return <span className="shrink-0 type-meta font-medium">{t("settings.recommended")}</span>
  if (state === "unsupported")
    return (
      <span className="shrink-0 type-meta text-muted-foreground">{t("settings.unsupported")}</span>
    )
  return <span className="shrink-0 type-meta text-muted-foreground">{t("settings.optional")}</span>
}

function UpgradeTag({ installation }: { installation: Schemas.Installation }) {
  const t = useT()
  if (installation.managedBy)
    return (
      <span className="type-meta text-muted-foreground">
        {t("common.managedBy", installation.managedBy)}
      </span>
    )
  const tested = installation.tested ?? ""
  switch (installation.upgrade) {
    case "behind-major":
    case "behind-minor":
    case "behind-patch":
      return (
        <StatusPill health="warn" size="sm">
          {t("cluster.tested", tested)}
        </StatusPill>
      )
    case "current":
      return (
        <StatusPill health="ok" size="sm">
          {t("cluster.testedVersion")}
        </StatusPill>
      )
    case "ahead":
      return (
        <StatusPill health="idle" size="sm">
          {t("cluster.newerThanTested")}
        </StatusPill>
      )
    case "unknown":
    case null:
    case undefined:
      return null
  }
}

const sourceKey = {
  k3s: "cluster.sourceK3s",
  rke2: "cluster.sourceRke2",
  zeabur: "cluster.sourceZeabur",
  "helm-chart": "cluster.sourceHelmChart",
  helm: "cluster.sourceHelm",
  other: "cluster.sourceOther",
} as const

function InstalledSection() {
  const t = useT()
  const capLabel = useCapabilityLabel()
  const capabilities = useQuery(capabilitiesQuery)
  const isDown = useDown()
  const items = capabilities.data?.items ?? []
  const installed = items.flatMap((c) => c.installations.map((i) => ({ capability: c.name, i })))
  return (
    <Section
      id="cluster-installed"
      title={t("settings.installed")}
      count={capabilities.data ? installed.length : undefined}
    >
      {capabilities.isPending ? (
        <LoadingRows label={t("common.loading")} rows={3} />
      ) : capabilities.isError ? (
        <Problem error={capabilities.error} />
      ) : installed.length === 0 ? (
        <p className="max-w-prose text-muted-foreground">{t("cluster.noComponents")}</p>
      ) : (
        <ul className="flex flex-col">
          {installed.map(({ capability, i }) => (
            <li
              key={`${capability}/${i.component}`}
              className="flex items-center gap-4 border-t border-rule py-2.5 first:border-t-0"
            >
              <span className="flex min-w-0 flex-1 flex-col">
                <span className="font-medium">{i.displayName}</span>
                <span className="type-meta text-muted-foreground">
                  {capLabel(capability)}
                  {i.source === "other" ? "" : `${t("common.sep")}${t(sourceKey[i.source])}`}
                </span>
              </span>
              <span className="flex min-w-0 flex-col items-end gap-1 sm:flex-row sm:items-center sm:gap-4">
                <span className="max-w-40 truncate type-mono sm:w-40" translate="no">
                  {i.version ?? <span className="text-muted-foreground">—</span>}
                </span>
                <span className="flex justify-end gap-1.5 sm:w-44">
                  {isDown(capability, i.component) ? (
                    <StatusPill health="fail" size="sm">
                      {t("status.failing")}
                    </StatusPill>
                  ) : null}
                  <UpgradeTag installation={i} />
                </span>
              </span>
            </li>
          ))}
        </ul>
      )}
    </Section>
  )
}
