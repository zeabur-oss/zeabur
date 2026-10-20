import { useQuery } from "@tanstack/react-query"
import { getRouteApi } from "@tanstack/react-router"
import { lazy, Suspense } from "react"
import BellIcon from "~icons/ph/bell"
import ChartBarIcon from "~icons/ph/chart-bar"
import ClockCounterClockwiseIcon from "~icons/ph/clock-counter-clockwise"
import CodeIcon from "~icons/ph/code"
import DotsThreeIcon from "~icons/ph/dots-three"
import GearSixIcon from "~icons/ph/gear-six"
import GlobeIcon from "~icons/ph/globe"
import HardDrivesIcon from "~icons/ph/hard-drives"
import InfoIcon from "~icons/ph/info"
import KeyIcon from "~icons/ph/key"
import TerminalWindowIcon from "~icons/ph/terminal-window"
import TextAlignLeftIcon from "~icons/ph/text-align-left"
import TrashIcon from "~icons/ph/trash"

import type { Schemas } from "@/api/client"
import { projectQuery } from "@/api/queries"
import { CopyButton } from "@/components/bits"
import { LoadingRows } from "@/components/page"
import { usePlanner } from "@/components/plan/planner"
import { Problem } from "@/components/problem"
import { refusedAmong, refusedElsewhere, RefusedKinds, RefusedSome } from "@/components/refused"
import { ServiceIcon } from "@/components/service-icon"
import { Button } from "@zeabur/design-system/components/button"
import {
  Menu,
  MenuContent,
  MenuItem,
  MenuSeparator,
  MenuTrigger,
} from "@zeabur/design-system/components/menu"
import { Skeleton } from "@zeabur/design-system/components/skeleton"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@zeabur/design-system/components/tabs"
import { Tooltip } from "@zeabur/design-system/components/tooltip"
import { useI18n, useT } from "@/i18n"
import type { Icon } from "@/lib/icon"
import { formatDate } from "@/lib/format"
import { cn } from "@/lib/utils"
import { edgesOf } from "@/model/connections"
import { eventsFor, serviceId, type Project, type Service } from "@/model/service"
import { inServiceTabs, refusedPlaces, useProjectEvents } from "@/routes/project/data"
import type { ServiceTab } from "@/routes/search"
import { EventsTab } from "@/routes/service/events"
import { MetricsTab } from "@/routes/service/metrics"
import { NetworkingTab } from "@/routes/service/networking"
import { OverviewTab } from "@/routes/service/overview"
import { ReleasesTab } from "@/routes/service/releases"
import { StorageTab } from "@/routes/service/storage"
import { VariablesTab } from "@/routes/service/variables"
import { YamlTab } from "@/routes/service/yaml"

const tabIcons: Record<ServiceTab, Icon> = {
  overview: InfoIcon,
  logs: TextAlignLeftIcon,
  variables: KeyIcon,
  metrics: ChartBarIcon,
  networking: GlobeIcon,
  storage: HardDrivesIcon,
  settings: GearSixIcon,
  events: BellIcon,
  releases: ClockCounterClockwiseIcon,
  terminal: TerminalWindowIcon,
  yaml: CodeIcon,
}

const everydayTabs: readonly ServiceTab[] = [
  "overview",
  "logs",
  "variables",
  "metrics",
  "networking",
  "storage",
  "settings",
]
const openedTabs: readonly ServiceTab[] = ["terminal", "yaml"]

export function serviceTabs(
  service: Pick<Service, "kind">,
  { events, open }: { events: number; open: ServiceTab },
): ServiceTab[] {
  const tabs = [...everydayTabs]
  if (events > 0 || open === "events") tabs.push("events")
  if (service.kind !== "Job" && service.kind !== "CronJob") tabs.push("releases")
  else if (open === "releases") tabs.push("releases")
  if (openedTabs.includes(open)) tabs.push(open)
  return tabs
}

const LogsTab = lazy(() => import("@/routes/service/logs").then((m) => ({ default: m.LogsTab })))
const TerminalTab = lazy(() =>
  import("@/routes/service/terminal").then((m) => ({ default: m.TerminalTab })),
)
const SettingsTab = lazy(() =>
  import("@/routes/service/settings").then((m) => ({ default: m.SettingsTab })),
)

const route = getRouteApi("/projects/$project/services/$kind/$name")

const workloadResource: Record<Service["kind"], Schemas["ResourceRef"]> = {
  Deployment: { group: "apps", version: "v1", resource: "deployments" },
  StatefulSet: { group: "apps", version: "v1", resource: "statefulsets" },
  DaemonSet: { group: "apps", version: "v1", resource: "daemonsets" },
  Job: { group: "batch", version: "v1", resource: "jobs" },
  CronJob: { group: "batch", version: "v1", resource: "cronjobs" },
}

export function ServicePage() {
  const t = useT()
  const { project, kind, name } = route.useParams()
  const view = useQuery(projectQuery(project))
  const service = view.data?.services.find((s) => serviceId(s) === `${kind}/${name}`)

  if (view.isPending)
    return (
      <Column>
        <LoadingRows label={t("common.loading")} rows={6} />
      </Column>
    )
  if (view.isError)
    return (
      <Column>
        <Problem error={view.error} />
      </Column>
    )
  if (!service)
    return (
      <Column>
        <p className="text-sm text-muted-foreground">{t("service.notFound")}</p>
      </Column>
    )
  return <ServiceView project={project} service={service} data={view.data} />
}

function ServiceView({
  project,
  service,
  data,
}: {
  project: string
  service: Service
  data: Project & {
    extensions?: Schemas["ProjectExtensions"]
    unreadable: Schemas["UnreadableView"][]
  }
}) {
  const t = useT()
  const { locale } = useI18n()
  const search = route.useSearch()
  const navigate = route.useNavigate()
  const events = useProjectEvents(project)
  const edges = edgesOf(data)
  const mine = eventsFor(events, service)
  const tab = search.tab ?? "overview"
  const setTab = (next: ServiceTab, pod?: string) =>
    void navigate({
      search: (previous) => ({
        ...previous,
        tab: next === "overview" ? undefined : next,
        pod:
          pod ??
          (next === "logs" || next === "terminal" || next === "events" ? previous.pod : undefined),
      }),
    })
  const setPod = (pod?: string) =>
    void navigate({ search: (previous) => ({ ...previous, pod: pod || undefined }), replace: true })
  const tabs = serviceTabs(service, { events: mine.length, open: tab })
  const unreadable = data.unreadable
  const restartable =
    service.kind === "Deployment" || service.kind === "StatefulSet" || service.kind === "DaemonSet"
  const planner = usePlanner(project)
  const deleter = usePlanner(
    project,
    () => void navigate({ to: "/projects/$project/services", params: { project }, search: {} }),
  )
  const planDeletion = () =>
    deleter.plan({
      title: t("title.delete", service.kind, service.name),
      changes: [{ op: "delete", resource: workloadResource[service.kind], name: service.name }],
    })
  const id = `${service.kind}/${service.name}`
  const wide = tab === "logs" || tab === "terminal"

  return (
    <Tabs
      value={tab}
      onValueChange={(value) => setTab(value as ServiceTab)}
      variant="page"
      className="flex min-h-0 flex-1 flex-col"
    >
      <header className="shrink-0 px-4 md:px-8">
        <div className="mx-auto flex w-full max-w-content flex-col">
          <div className="flex items-start gap-x-3 py-4 md:py-6">
            <span className="flex size-10 shrink-0 items-center justify-center rounded-xl border border-main bg-card">
              <ServiceIcon service={service} size="lg" />
            </span>
            <div className="flex min-w-0 flex-1 flex-col gap-1">
              <h1 className="min-w-0 truncate type-name" translate="no">
                {service.displayName ?? service.name}
              </h1>
              <div className="flex min-w-0 flex-col gap-0.5 text-xs text-secondary">
                {service.created ? (
                  <span>{t("service.created", formatDate(service.created, locale))}</span>
                ) : null}
                <span className="flex min-w-0 items-center gap-1">
                  <span className="truncate font-mono" translate="no">
                    {id}
                  </span>
                  <CopyButton value={id} />
                </span>
              </div>
            </div>
            <Menu>
              <MenuTrigger
                render={<Button variant="outline" size="icon-sm" aria-label={t("service.more")} />}
              >
                <DotsThreeIcon aria-hidden="true" />
              </MenuTrigger>
              <MenuContent>
                <MenuItem onClick={() => setTab("terminal")}>
                  <TerminalWindowIcon aria-hidden="true" />
                  {t("service.openTerminal")}
                </MenuItem>
                <MenuItem onClick={() => setTab("yaml")}>
                  <CodeIcon aria-hidden="true" />
                  {t("service.viewYaml")}
                </MenuItem>
                <MenuSeparator />
                {service.provenance.tool ? (
                  <Tooltip content={t("common.managedByDetail", service.provenance.tool)}>
                    <MenuItem disabled>
                      <TrashIcon aria-hidden="true" />
                      {t("service.delete")}
                    </MenuItem>
                  </Tooltip>
                ) : (
                  <MenuItem onClick={planDeletion}>
                    <TrashIcon aria-hidden="true" />
                    {t("service.delete")}
                  </MenuItem>
                )}
              </MenuContent>
            </Menu>
          </div>
          <RefusedSome unreadable={refusedElsewhere(unreadable, inServiceTabs)} />
          <div className="pt-1 pb-3">
            <TabsList aria-label={t("service.tabs")} className="w-full">
              {tabs.map((value) => {
                const TabIcon = tabIcons[value]
                return (
                  <TabsTrigger key={value} value={value} className="flex-1">
                    <TabIcon aria-hidden="true" />
                    {t(`tab.${value}`)}
                  </TabsTrigger>
                )
              })}
            </TabsList>
          </div>
        </div>
      </header>
      <div className={cn("flex min-h-0 flex-1 flex-col px-4 md:px-8", wide ? "pb-4" : "pb-16")}>
        <div
          className={cn(
            "mx-auto flex w-full max-w-content flex-col pt-3",
            wide && "min-h-0 flex-1",
          )}
        >
          <TabsContent value="overview">
            <OverviewTab
              project={project}
              service={service}
              services={data.services}
              network={data}
              edges={edges}
              events={mine}
              onTab={setTab}
              onRestart={
                restartable
                  ? () =>
                      planner.plan({
                        title: t("title.restart", service.name),
                        changes: [
                          { op: "restart", workload: { kind: service.kind, name: service.name } },
                        ],
                      })
                  : undefined
              }
            />
          </TabsContent>
          <TabsContent value="logs" className="flex min-h-0 flex-1 flex-col">
            {tab === "logs" ? (
              <Suspense fallback={<Skeleton variant="card" className="min-h-96 w-full flex-1" />}>
                <LogsTab
                  project={project}
                  service={service}
                  pod={search.pod}
                  onPodChange={setPod}
                />
              </Suspense>
            ) : null}
          </TabsContent>
          <TabsContent value="terminal" className="flex min-h-0 flex-1 flex-col">
            {tab === "terminal" ? (
              <Suspense fallback={<Skeleton variant="card" className="min-h-96 w-full flex-1" />}>
                <TerminalTab
                  project={project}
                  service={service}
                  pod={search.pod}
                  onPodChange={setPod}
                />
              </Suspense>
            ) : null}
          </TabsContent>
          <TabsContent value="variables">
            <VariablesTab
              project={project}
              service={service}
              edges={edges.filter((e) => e.from === serviceId(service))}
              refused={refusedAmong(unreadable, refusedPlaces.config)}
            />
          </TabsContent>
          <TabsContent value="networking">
            <NetworkingTab
              project={project}
              service={service}
              services={data.services}
              network={data}
              reachability={data.extensions?.networkPolicy ?? undefined}
              unreadable={unreadable}
            />
          </TabsContent>
          <TabsContent value="storage">
            <div className="flex flex-col gap-4">
              <RefusedKinds unreadable={refusedAmong(unreadable, refusedPlaces.claims)} />
              <StorageTab project={project} service={service} />
            </div>
          </TabsContent>
          <TabsContent value="metrics">
            {tab === "metrics" ? <MetricsTab project={project} service={service} /> : null}
          </TabsContent>
          <TabsContent value="settings">
            {tab === "settings" ? (
              <Suspense fallback={<LoadingRows label={t("common.loading")} rows={6} />}>
                <SettingsTab project={project} service={service} network={data} />
              </Suspense>
            ) : null}
          </TabsContent>
          <TabsContent value="events">
            <div className="flex flex-col gap-4">
              <RefusedKinds unreadable={refusedAmong(unreadable, refusedPlaces.events)} />
              <EventsTab events={mine} pod={search.pod} onClearPod={() => setPod(undefined)} />
            </div>
          </TabsContent>
          <TabsContent value="releases">
            {tab === "releases" ? <ReleasesTab project={project} service={service} /> : null}
          </TabsContent>
          <TabsContent value="yaml">
            {tab === "yaml" ? <YamlTab project={project} service={service} /> : null}
          </TabsContent>
        </div>
      </div>
      {planner.dialog}
      {deleter.dialog}
    </Tabs>
  )
}

function Column({ children }: { children: React.ReactNode }) {
  return (
    <div className="px-4 py-6 md:px-8 md:py-8">
      <div className="mx-auto w-full max-w-content">{children}</div>
    </div>
  )
}
