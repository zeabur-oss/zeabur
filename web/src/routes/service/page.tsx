import { useQuery } from "@tanstack/react-query"
import { getRouteApi } from "@tanstack/react-router"
import { useState } from "react"
import ArrowClockwiseIcon from "~icons/ph/arrow-clockwise"
import BracketsCurlyIcon from "~icons/ph/brackets-curly"
import ClockCounterClockwiseIcon from "~icons/ph/clock-counter-clockwise"
import DotsThreeIcon from "~icons/ph/dots-three"
import GearSixIcon from "~icons/ph/gear-six"
import GlobeIcon from "~icons/ph/globe"
import InfoIcon from "~icons/ph/info"
import TextAlignLeftIcon from "~icons/ph/text-align-left"
import TrashIcon from "~icons/ph/trash"

import type { Schemas } from "@/api/client"
import { projectQuery } from "@/api/queries"
import { Freshness } from "@/routes/project/page"
import { CopyButton } from "@/components/bits"
import { LoadingRows } from "@/components/page"
import { describeDraft, DraftProvider, useDraftState, type Draft } from "@/components/plan/draft"
import { usePlanner } from "@/components/plan/planner"
import { Problem } from "@/components/problem"
import { refusedAmong, refusedElsewhere, RefusedSome } from "@/components/refused"
import { ServiceIcon } from "@/components/service-icon"
import { ServiceStatus } from "@/components/status"
import { Button } from "@zeabur/design-system/components/button"
import { DraftBar } from "@zeabur/design-system/components/draft-bar"
import { usePresence } from "@zeabur/design-system/components/presence"
import {
  Menu,
  MenuContent,
  MenuItem,
  MenuSeparator,
  MenuTrigger,
} from "@zeabur/design-system/components/menu"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@zeabur/design-system/components/tabs"
import { Tooltip } from "@zeabur/design-system/components/tooltip"
import { useI18n, useT } from "@/i18n"
import type { Icon } from "@/lib/icon"
import { formatDate } from "@/lib/format"
import { cn } from "@/lib/utils"
import { edgesOf } from "@/model/connections"
import { serviceLink } from "@/lib/place"
import { eventsFor, serviceId, statusOf, type Project, type Service } from "@/model/service"
import { inServiceTabs, refusedPlaces, useProjectEvents } from "@/routes/project/data"
import {
  serviceTabs,
  serviceViews,
  type DraftPart,
  type ServicePage as Page,
  type ServicePlace,
  type ServiceTab,
  type ServiceView as View,
} from "@/routes/search"
import { DeploymentsTab } from "@/routes/service/deployments"
import { NetworkingTab } from "@/routes/service/networking"
import { OverviewTab } from "@/routes/service/overview"
import { Managed, PendingMark, SettingsTab } from "@/routes/service/settings-tab"
import { VariablesTab } from "@/routes/service/variables"
import { Logs, ServiceViewPanel, viewIcon, viewLabel } from "@/routes/service/views"

const tabIcons: Record<ServiceTab, Icon> = {
  overview: InfoIcon,
  logs: TextAlignLeftIcon,
  deployments: ClockCounterClockwiseIcon,
  networking: GlobeIcon,
  variables: BracketsCurlyIcon,
  settings: GearSixIcon,
}

/** The tab that holds each part a pending edit belongs to. */
const tabOfPart: Record<DraftPart, ServiceTab> = {
  networking: "networking",
  variables: "variables",
  compute: "settings",
  "health-checks": "settings",
  storage: "settings",
}

/** Tabs whose forms keep what was typed while another tab is shown. */
const editing: readonly Page[] = ["networking", "variables", "settings"]

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
  const tab: Page = search.tab ?? "overview"
  const view = (serviceViews as readonly string[]).includes(tab) ? (tab as View) : undefined
  const fresh = useQuery(projectQuery(project))
  const go = (place: ServicePlace, pod?: string) =>
    void navigate(serviceLink(project, service, { tab: place, pod }))
  const setTab = (next: Page) =>
    void navigate({
      search: (previous) => ({
        ...previous,
        tab: next === "overview" ? undefined : next,
        pod: next === "logs" || next === "terminal" || next === "events" ? previous.pod : undefined,
      }),
      hash: "",
    })
  const setPod = (pod?: string) =>
    void navigate({ search: (previous) => ({ ...previous, pod: pod || undefined }), replace: true })
  const unreadable = data.unreadable
  const restartable =
    service.kind === "Deployment" || service.kind === "StatefulSet" || service.kind === "DaemonSet"
  const draft = useDraftState()
  const pending = usePresence<HTMLDivElement>(draft.count > 0)
  const reviewer = usePlanner(project, draft.applied, true)
  const planner = usePlanner(project)
  const deleter = usePlanner(
    project,
    () => void navigate({ to: "/projects/$project", params: { project }, search: {} }),
  )
  // A form keeps what was typed while another tab is shown, until the draft is applied or dropped.
  const [opened, setOpened] = useState<readonly Page[]>([tab])
  if (!opened.includes(tab)) setOpened([...opened, tab])
  const planDeletion = () =>
    deleter.plan({
      title: t("title.delete", service.kind, service.name),
      changes: [{ op: "delete", resource: workloadResource[service.kind], name: service.name }],
    })
  const restart = restartable
    ? () =>
        planner.plan({
          title: t("title.restart", service.name),
          changes: [{ op: "restart", workload: { kind: service.kind, name: service.name } }],
        })
    : undefined
  const id = `${service.kind}/${service.name}`
  const wide = tab === "logs" || tab === "terminal"
  const pendingIn = new Set(draft.parts.map((part) => tabOfPart[part]))
  const kept = (value: Page) => editing.includes(value) && opened.includes(value)

  return (
    <DraftProvider draft={draft}>
      <Tabs
        value={tab}
        onValueChange={(value) => setTab(value as Page)}
        variant="page"
        className="flex min-h-0 flex-1 flex-col"
      >
        <header className="shrink-0 px-4 md:px-8">
          <div className="mx-auto flex w-full max-w-content flex-col">
            <div className="flex flex-wrap items-start gap-x-3 gap-y-3 py-4 md:py-6">
              <span className="flex size-10 shrink-0 items-center justify-center rounded-xl border border-main bg-card">
                <ServiceIcon service={service} size="lg" />
              </span>
              <div className="flex min-w-0 flex-1 flex-col gap-1">
                <div className="flex min-w-0 flex-wrap items-center gap-x-2.5 gap-y-1">
                  <h1 className="min-w-0 truncate type-name" translate="no">
                    {service.displayName ?? service.name}
                  </h1>
                  <ServiceStatus status={statusOf(service)} counts swap />
                </div>
                <div className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-0.5 text-xs text-secondary">
                  <span className="flex min-w-0 items-center gap-1">
                    <span className="truncate font-mono" translate="no">
                      {id}
                    </span>
                    <CopyButton value={id} />
                  </span>
                  {service.created ? (
                    <span>{t("service.created", formatDate(service.created, locale))}</span>
                  ) : null}
                  {fresh.isRefetchError ? (
                    <Freshness updatedAt={fresh.dataUpdatedAt} failed />
                  ) : null}
                </div>
              </div>
              <div className="flex shrink-0 items-center gap-1.5">
                {restart ? (
                  <Button variant="outline" size="sm" onClick={restart}>
                    <ArrowClockwiseIcon aria-hidden="true" data-icon="inline-start" />
                    {t("ops.restart")}
                  </Button>
                ) : null}
                <Menu>
                  <MenuTrigger
                    render={
                      <Button variant="outline" size="icon-sm" aria-label={t("service.more")} />
                    }
                  >
                    <DotsThreeIcon aria-hidden="true" />
                  </MenuTrigger>
                  <MenuContent>
                    {serviceViews.map((value) => {
                      const ViewIcon = viewIcon[value]
                      return (
                        <MenuItem key={value} onClick={() => go(value)}>
                          <ViewIcon aria-hidden="true" />
                          {t(viewLabel[value])}
                        </MenuItem>
                      )
                    })}
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
            </div>
            <RefusedSome unreadable={refusedElsewhere(unreadable, inServiceTabs)} />
            <div className="pt-1 pb-3">
              <TabsList aria-label={t("service.tabs")} className="w-full">
                {serviceTabs.map((value) => {
                  const TabIcon = tabIcons[value]
                  return (
                    <TabsTrigger key={value} value={value} className="flex-1">
                      <TabIcon aria-hidden="true" className="max-sm:hidden" />
                      {t(`tab.${value}`)}
                      {pendingIn.has(value) ? <PendingMark /> : null}
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
              {tab === "overview" ? (
                <OverviewTab
                  project={project}
                  service={service}
                  services={data.services}
                  edges={edges}
                  events={mine}
                  onTab={go}
                />
              ) : null}
            </TabsContent>
            <TabsContent
              value="logs"
              className={tab === "logs" ? "flex min-h-0 flex-col" : undefined}
            >
              {tab === "logs" ? (
                <Logs project={project} service={service} pod={search.pod} onPodChange={setPod} />
              ) : null}
            </TabsContent>
            <TabsContent value="deployments">
              {tab === "deployments" ? (
                <DeploymentsTab
                  project={project}
                  service={service}
                  events={mine}
                  onEvents={() => go("events")}
                />
              ) : null}
            </TabsContent>
            <TabsContent value="networking" keepMounted={kept("networking")}>
              {tab === "networking" || kept("networking") ? (
                <div key={draft.generation} className="flex flex-col gap-6">
                  <Managed tool={service.provenance.tool} />
                  <NetworkingTab
                    project={project}
                    service={service}
                    services={data.services}
                    network={data}
                    reachability={data.extensions?.networkPolicy ?? undefined}
                    unreadable={unreadable}
                  />
                </div>
              ) : null}
            </TabsContent>
            <TabsContent value="variables" keepMounted={kept("variables")}>
              {tab === "variables" || kept("variables") ? (
                <div key={draft.generation} className="flex flex-col gap-6">
                  <Managed tool={service.provenance.tool} />
                  <VariablesTab
                    project={project}
                    service={service}
                    edges={edges.filter((e) => e.from === serviceId(service))}
                    refused={refusedAmong(unreadable, refusedPlaces.config)}
                  />
                </div>
              ) : null}
            </TabsContent>
            <TabsContent value="settings" keepMounted={kept("settings")}>
              {tab === "settings" || kept("settings") ? (
                <SettingsTab
                  project={project}
                  service={service}
                  network={data}
                  unreadable={unreadable}
                />
              ) : null}
            </TabsContent>
            {serviceViews.map((value) => (
              <TabsContent
                key={value}
                value={value}
                className={
                  value === "terminal" && tab === value ? "flex min-h-0 flex-1 flex-col" : undefined
                }
              >
                {view === value ? (
                  <ServiceViewPanel
                    project={project}
                    service={service}
                    view={value}
                    pod={search.pod}
                    events={mine}
                    unreadable={unreadable}
                    onPod={setPod}
                    onClose={() => setTab("overview")}
                  />
                ) : null}
              </TabsContent>
            ))}
            {pending.mounted ? (
              <PendingChanges
                draft={draft}
                presence={pending.props}
                onReview={() =>
                  reviewer.plan({
                    title: t("draft.title", service.name),
                    changes: draft.changes,
                    restartAfter: draft.restartAfter || undefined,
                    service: { kind: service.kind, name: service.name },
                  })
                }
              />
            ) : null}
          </div>
        </div>
        {planner.dialog}
        {deleter.dialog}
        {reviewer.dialog}
      </Tabs>
    </DraftProvider>
  )
}

function PendingChanges({
  draft,
  presence,
  onReview,
}: {
  draft: Draft
  presence: ReturnType<typeof usePresence<HTMLDivElement>>["props"]
  onReview: () => void
}) {
  const t = useT()
  const detail = describeDraft(t, draft)
  // While it fades out after Discard or Apply, it keeps saying what it held.
  const [shown, setShown] = useState({ count: draft.count, detail })
  if (draft.count > 0 && (draft.count !== shown.count || detail !== shown.detail))
    setShown({ count: draft.count, detail })
  // The wrapper sticks in the bar's place, so it can fade the bar in and out.
  return (
    <div {...presence} className="sticky bottom-3 z-10 mt-6 overlay-motion">
      <DraftBar
        label={t("draft.count", shown.count)}
        detail={shown.detail}
        actions={
          <>
            <Button variant="ghost" size="sm" onClick={draft.discard}>
              {t("draft.discard")}
            </Button>
            <Button size="sm" onClick={onReview}>
              {t("draft.review")}
            </Button>
          </>
        }
      />
    </div>
  )
}

function Column({ children }: { children: React.ReactNode }) {
  return (
    <div className="px-4 py-6 md:px-8 md:py-8">
      <div className="mx-auto w-full max-w-content">{children}</div>
    </div>
  )
}
