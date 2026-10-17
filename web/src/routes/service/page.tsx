// The service page: the way back to its project's Overview, who it is (icon, name, kind, status,
// image; each said once), the operations with the cluster they act on next to them, and the tabs Overview, Logs, Terminal, Variables,
// Networking, Storage, Settings, Events, Releases and YAML, the open tab and pod in the URL. Restart and
// Delete, like every write, open a plan to read before applying; a deleted service's page goes
// back to the project's services.
import { useQuery } from "@tanstack/react-query"
import { getRouteApi, Link } from "@tanstack/react-router"
import { lazy, Suspense } from "react"
import ArrowClockwiseIcon from "~icons/ph/arrow-clockwise"
import ArrowLeftIcon from "~icons/ph/arrow-left"
import CaretDownIcon from "~icons/ph/caret-down"
import DotsThreeIcon from "~icons/ph/dots-three"
import GitBranchIcon from "~icons/ph/git-branch"
import TerminalIcon from "~icons/ph/terminal"
import TextAlignLeftIcon from "~icons/ph/text-align-left"
import TrashIcon from "~icons/ph/trash"

import type { Schemas } from "@/api/client"
import { projectQuery } from "@/api/queries"
import { CopyButton } from "@/components/bits"
import { Identity } from "@/components/identity"
import { LoadingRows } from "@/components/page"
import { usePlanner } from "@/components/plan/planner"
import { Problem } from "@/components/problem"
import { refusedAmong, refusedElsewhere, RefusedKinds, RefusedSome } from "@/components/refused"
import { ServiceIcon } from "@/components/service-icon"
import { ServiceStatus } from "@/components/status"
import { Button } from "@/components/ui/button"
import { Menu, MenuContent, MenuItem, MenuTrigger } from "@/components/ui/menu"
import { Skeleton } from "@/components/ui/skeleton"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { Tooltip } from "@/components/ui/tooltip"
import { useT } from "@/i18n"
import { useProjectSummary } from "@/lib/project-name"
import { cn } from "@/lib/utils"
import { edgesOf } from "@/model/connections"
import {
  eventsFor,
  imageTag,
  kubectl,
  serviceId,
  statusOf,
  volumesOf,
  type Project,
  type Service,
} from "@/model/service"
import { inServiceTabs, refusedPlaces, useProjectEvents } from "@/routes/project/data"
import { serviceTabs, type ServiceTab } from "@/routes/search"
import { EventsTab } from "@/routes/service/events"
import { NetworkingTab } from "@/routes/service/networking"
import { OverviewTab } from "@/routes/service/overview"
import { ReleasesTab } from "@/routes/service/releases"
import { StorageTab } from "@/routes/service/storage"
import { VariablesTab } from "@/routes/service/variables"
import { YamlTab } from "@/routes/service/yaml"

// The log viewer (virtual list, ANSI parsing, streaming) loads when the Logs tab first opens.
const LogsTab = lazy(() => import("@/routes/service/logs").then((m) => ({ default: m.LogsTab })))
// The terminal (xterm.js) loads when the Terminal tab first opens.
const TerminalTab = lazy(() =>
  import("@/routes/service/terminal").then((m) => ({ default: m.TerminalTab })),
)
// The settings forms and their form library load when the Settings tab first opens.
const SettingsTab = lazy(() =>
  import("@/routes/service/settings").then((m) => ({ default: m.SettingsTab })),
)

const route = getRouteApi("/projects/$project/services/$kind/$name")

/** The resource of each workload kind, as a deletion names it. */
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
      <div className="px-4 py-6 md:px-8">
        <LoadingRows label={t("common.loading")} rows={6} />
      </div>
    )
  if (view.isError)
    return (
      <div className="px-4 py-6 md:px-8">
        <Problem error={view.error} />
      </div>
    )
  if (!service)
    return (
      <p className="px-4 py-6 text-sm text-muted-foreground md:px-8">{t("service.notFound")}</p>
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
  const search = route.useSearch()
  const navigate = route.useNavigate()
  const events = useProjectEvents(project)
  const edges = edgesOf(data)
  const mine = eventsFor(events, service)
  const tab = search.tab ?? "overview"
  const status = statusOf(service)
  const projectName = useProjectSummary(project)?.displayName ?? project
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
  const counts: Partial<Record<ServiceTab, number>> = {
    variables: service.containers.reduce((n, c) => n + c.env.length, 0),
    networking: service.routes.length + service.endpoints.length,
    storage: volumesOf(service).length,
    events: mine.length,
  }
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

  return (
    <Tabs
      value={tab}
      onValueChange={(value) => setTab(value as ServiceTab)}
      variant="page"
      className="flex min-h-0 flex-1 flex-col"
    >
      <header className="flex shrink-0 flex-col gap-3 border-b px-4 pt-4 md:px-8">
        <Link
          to="/projects/$project"
          params={{ project }}
          className="inline-flex w-fit items-center gap-1 rounded-sm text-sm text-muted-foreground outline-none hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring"
          aria-label={t("service.backTo", projectName)}
        >
          <ArrowLeftIcon aria-hidden="true" className="size-3.5" />
          <span translate="no">{projectName}</span>
        </Link>
        <div className="flex flex-wrap items-start gap-x-3 gap-y-2">
          <ServiceIcon service={service} size="lg" />
          <div className="flex min-w-0 flex-1 flex-col gap-1">
            <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1">
              <h1 className="truncate text-xl font-semibold tracking-tight" translate="no">
                {service.displayName ?? service.name}
              </h1>
              <ServiceStatus status={status} counts />
            </div>
            <div className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
              <span translate="no">{service.kind}</span>
              {service.image ? (
                <span className="flex min-w-0 items-center font-mono" title={service.image}>
                  <span className="truncate" translate="no">
                    {service.image.replace(/[:@][^:/@]+$/, "")}
                  </span>
                  <span className="rounded bg-muted px-1 text-foreground" translate="no">
                    :{imageTag(service.image)}
                  </span>
                  <CopyButton value={service.image} />
                </span>
              ) : null}
              {service.provenance.tool ? (
                <Tooltip content={t("common.managedByDetail", service.provenance.tool)}>
                  <span className="inline-flex cursor-help items-center gap-1">
                    <GitBranchIcon className="size-3" aria-hidden="true" />
                    {t("common.managedBy", service.provenance.tool)}
                  </span>
                </Tooltip>
              ) : null}
            </div>
          </div>
          <div className="flex w-full shrink-0 items-center gap-1.5 sm:w-auto">
            <Button variant="outline" size="sm" onClick={() => setTab("logs")}>
              <TextAlignLeftIcon aria-hidden="true" data-icon="inline-start" />
              {t("tab.logs")}
            </Button>
            <Menu>
              <MenuTrigger render={<Button variant="outline" size="sm" />}>
                <TerminalIcon aria-hidden="true" data-icon="inline-start" />
                kubectl
                <CaretDownIcon aria-hidden="true" data-icon="inline-end" />
              </MenuTrigger>
              <MenuContent className="w-80">
                {(["yaml", "describe", "logs", "restart"] as const).map((verb) => (
                  <MenuItem
                    key={verb}
                    onClick={() =>
                      void navigator.clipboard.writeText(kubectl(service, verb)).catch(() => {})
                    }
                  >
                    <TerminalIcon aria-hidden="true" />
                    <span className="truncate font-mono text-xs">{kubectl(service, verb)}</span>
                  </MenuItem>
                ))}
              </MenuContent>
            </Menu>
            <Menu>
              <MenuTrigger
                render={<Button variant="outline" size="icon-sm" aria-label={t("service.more")} />}
              >
                <DotsThreeIcon aria-hidden="true" />
              </MenuTrigger>
              <MenuContent>
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
        </div>
        {restartable ? (
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5">
            <Button
              variant="outline"
              size="xs"
              onClick={() =>
                planner.plan({
                  title: t("title.restart", service.name),
                  changes: [
                    { op: "restart", workload: { kind: service.kind, name: service.name } },
                  ],
                })
              }
            >
              <ArrowClockwiseIcon aria-hidden="true" data-icon="inline-start" />
              {t("ops.restart")}
            </Button>
            {/* The back link names the project, the title the service: the cluster is what is
                left to say about where Restart acts. */}
            <Identity />
          </div>
        ) : null}
        <RefusedSome unreadable={refusedElsewhere(unreadable, inServiceTabs)} />
        <div className="-mx-1 overflow-x-auto scrollbar-none">
          <TabsList variant="line" aria-label={t("service.tabs")} className="h-9">
            {serviceTabs.map((value) => (
              <TabsTrigger key={value} value={value}>
                {t(`tab.${value}`)}
                {counts[value] ? (
                  <span className="rounded-full bg-muted px-1.5 type-nano font-medium text-muted-foreground tabular-nums">
                    {counts[value]}
                  </span>
                ) : null}
              </TabsTrigger>
            ))}
          </TabsList>
        </div>
      </header>
      <div
        className={cn(
          "min-h-0 flex-1 px-4 py-5 md:px-8",
          tab === "logs" || tab === "terminal" ? "flex flex-col" : "pb-16",
        )}
      >
        <TabsContent value="overview">
          <OverviewTab
            project={project}
            service={service}
            services={data.services}
            edges={edges}
            events={mine}
            onTab={setTab}
          />
        </TabsContent>
        <TabsContent value="logs" className="flex min-h-0 flex-1 flex-col">
          {tab === "logs" ? (
            <Suspense fallback={<Skeleton variant="card" className="min-h-96 w-full flex-1" />}>
              <LogsTab project={project} service={service} pod={search.pod} onPodChange={setPod} />
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
        <TabsContent value="settings">
          {tab === "settings" ? (
            <Suspense fallback={<LoadingRows label={t("common.loading")} rows={6} />}>
              <SettingsTab project={project} service={service} />
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
      {planner.dialog}
      {deleter.dialog}
    </Tabs>
  )
}
