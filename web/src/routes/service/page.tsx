import { useQuery } from "@tanstack/react-query"
import { getRouteApi, Link } from "@tanstack/react-router"
import { useState } from "react"
import ArrowClockwiseIcon from "~icons/ph/arrow-clockwise"
import ClockCounterClockwiseIcon from "~icons/ph/clock-counter-clockwise"
import CodeIcon from "~icons/ph/code"
import DotsThreeIcon from "~icons/ph/dots-three"
import FileTextIcon from "~icons/ph/file-text"
import MagnifyingGlassIcon from "~icons/ph/magnifying-glass"
import PulseIcon from "~icons/ph/pulse"
import SlidersHorizontalIcon from "~icons/ph/sliders-horizontal"
import TerminalWindowIcon from "~icons/ph/terminal-window"
import TrashIcon from "~icons/ph/trash"

import type { Schemas } from "@/api/client"
import { projectQuery } from "@/api/queries"
import { Freshness } from "@/routes/project/page"
import { CopyButton } from "@/components/bits"
import { LoadingRows } from "@/components/page"
import { describeDraft, DraftProvider, useDraftState, type Draft } from "@/components/plan/draft"
import { usePlanner } from "@/components/plan/planner"
import { Problem } from "@/components/problem"
import { refusedElsewhere, RefusedSome } from "@/components/refused"
import { ServiceIcon } from "@/components/service-icon"
import { ServiceStatus } from "@/components/status"
import { Button, buttonVariants } from "@zeabur/design-system/components/button"
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
import { eventsFor, isTask, serviceId, statusOf, type Project, type Service } from "@/model/service"
import { inServiceTabs, useProjectEvents } from "@/routes/project/data"
import {
  serviceSections,
  type InspectView,
  type ServicePlace,
  type ServiceSection,
} from "@/routes/search"
import { ChangesSection } from "@/routes/service/changes"
import { ConfigureSection } from "@/routes/service/configure"
import { InspectSection } from "@/routes/service/inspect"
import { StatusSection } from "@/routes/service/status"

const sectionIcons: Record<ServiceSection, Icon> = {
  status: PulseIcon,
  changes: ClockCounterClockwiseIcon,
  configure: SlidersHorizontalIcon,
  inspect: MagnifyingGlassIcon,
}

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
  const tab = search.tab ?? "status"
  const view = search.view ?? "logs"
  const fresh = useQuery(projectQuery(project))
  const go = (place: ServicePlace, pod?: string) =>
    void navigate(serviceLink(project, service, { tab: place, pod }))
  const setTab = (next: ServiceSection) =>
    void navigate({
      search: (previous) => ({
        ...previous,
        tab: next === "status" ? undefined : next,
        view: next === "inspect" ? previous.view : undefined,
        pod: next === "inspect" ? previous.pod : undefined,
      }),
      hash: "",
    })
  const setView = (next: InspectView) =>
    void navigate({ search: (previous) => ({ ...previous, tab: "inspect", view: next }) })
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
  const wide = tab === "inspect" && (view === "logs" || view === "terminal")
  const runs = !isTask(service.kind) && service.pods.some((p) => p.ready)
  const quick = buttonVariants({ variant: "outline", size: "sm" })

  return (
    <DraftProvider draft={draft}>
      <Tabs
        value={tab}
        onValueChange={(value) => setTab(value as ServiceSection)}
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
                <Link {...serviceLink(project, service, { tab: "logs" })} className={quick}>
                  <FileTextIcon aria-hidden="true" data-icon="inline-start" />
                  {t("tab.logs")}
                </Link>
                {runs ? (
                  <Link
                    {...serviceLink(project, service, { tab: "terminal" })}
                    className={cn(quick, "max-sm:hidden")}
                  >
                    <TerminalWindowIcon aria-hidden="true" data-icon="inline-start" />
                    {t("tab.terminal")}
                  </Link>
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
                    {restart ? (
                      <MenuItem onClick={restart}>
                        <ArrowClockwiseIcon aria-hidden="true" />
                        {t("ops.restart")}
                      </MenuItem>
                    ) : null}
                    <MenuItem onClick={() => go("terminal")}>
                      <TerminalWindowIcon aria-hidden="true" />
                      {t("service.openTerminal")}
                    </MenuItem>
                    <MenuItem onClick={() => go("yaml")}>
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
            </div>
            <RefusedSome unreadable={refusedElsewhere(unreadable, inServiceTabs)} />
            <div className="pt-1 pb-3">
              <TabsList aria-label={t("service.tabs")} className="w-full">
                {serviceSections.map((value) => {
                  const SectionIcon = sectionIcons[value]
                  return (
                    <TabsTrigger key={value} value={value} className="flex-1">
                      <SectionIcon aria-hidden="true" />
                      {t(`section.${value}`)}
                      {value === "configure" && draft.count > 0 ? (
                        <span className="tabular-nums text-progress-strong">{draft.count}</span>
                      ) : null}
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
            <TabsContent value="status">
              <StatusSection
                project={project}
                service={service}
                services={data.services}
                edges={edges}
                events={mine}
                onTab={go}
                onRestart={restart}
              />
            </TabsContent>
            <TabsContent value="changes">
              {tab === "changes" ? (
                <ChangesSection
                  project={project}
                  service={service}
                  events={mine}
                  onEvents={() => go("events")}
                />
              ) : null}
            </TabsContent>
            <TabsContent value="configure">
              {tab === "configure" ? (
                <ConfigureSection
                  project={project}
                  service={service}
                  services={data.services}
                  network={data}
                  edges={edges}
                  reachability={data.extensions?.networkPolicy ?? undefined}
                  unreadable={unreadable}
                />
              ) : null}
            </TabsContent>
            <TabsContent
              value="inspect"
              className={wide ? "flex min-h-0 flex-1 flex-col" : undefined}
            >
              {tab === "inspect" ? (
                <InspectSection
                  project={project}
                  service={service}
                  view={view}
                  pod={search.pod}
                  events={mine}
                  unreadable={unreadable}
                  onView={setView}
                  onPod={setPod}
                />
              ) : null}
            </TabsContent>
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
