import { useQuery } from "@tanstack/react-query"
import { toast } from "sonner"
import { getRouteApi, useBlocker } from "@tanstack/react-router"
import { useLayoutEffect, useRef, useState, type ReactNode } from "react"
import ArrowClockwiseIcon from "~icons/ph/arrow-clockwise"
import CaretRightIcon from "~icons/ph/caret-right"
import BracketsCurlyIcon from "~icons/ph/brackets-curly"
import ChartBarIcon from "~icons/ph/chart-bar"
import ClockCounterClockwiseIcon from "~icons/ph/clock-counter-clockwise"
import FolderIcon from "~icons/ph/folder"
import DotsThreeIcon from "~icons/ph/dots-three"
import GearSixIcon from "~icons/ph/gear-six"
import GlobeIcon from "~icons/ph/globe"
import HammerIcon from "~icons/ph/hammer"
import InfoIcon from "~icons/ph/info"
import TerminalWindowIcon from "~icons/ph/terminal-window"
import TextAlignLeftIcon from "~icons/ph/text-align-left"
import CopyIcon from "~icons/ph/copy"
import TrashIcon from "~icons/ph/trash"

import type { Schemas } from "@/api/client"
import { useLogsReadable } from "@/api/logs-access"
import { evidenceQuery, projectQuery, queryClient } from "@/api/queries"
import { Freshness } from "@/components/freshness"
import { LoadingRows } from "@/components/page"
import { describeDraft, DraftProvider, useDraftState, type Draft } from "@/components/plan/draft"
import { GatedMenuItem, GateTip, useGates } from "@/components/gate"
import { usePlanner } from "@/components/plan/planner"
import { Problem } from "@/components/problem"
import { refusedAmong, refusedElsewhere, RefusedSome } from "@/components/refused"
import { ServiceIcon } from "@/components/service-icon"
import { ServiceStatus } from "@/components/status"
import { Button } from "@zeabur/design-system/components/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "@zeabur/design-system/components/dialog"
import { DraftBar } from "@zeabur/design-system/components/draft-bar"
import { PageColumn } from "@zeabur/design-system/components/page-column"
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
import { useI18n, useT, writeSentence } from "@/i18n"
import type { Icon } from "@/lib/icon"
import { formatDate } from "@/lib/format"
import { cn } from "@/lib/utils"
import { edgesOf } from "@/model/connections"
import { openShell } from "@/lib/dock"
import { serviceLink } from "@/lib/place"
import { writeBundle } from "@/model/bundle"
import { eventsFor, isTask, serviceId, statusOf, type Project, type Service } from "@/model/service"
import { inServiceTabs, refusedPlaces, useProjectEvents } from "@/routes/project/data"
import {
  serviceTabs,
  type DraftPart,
  type ServiceMode,
  type ServicePlace,
  type ServiceTab as Page,
} from "@/routes/search"
import { BuildsTab, useServiceBuilds } from "@/routes/service/builds"
import { DeploymentsTab } from "@/routes/service/deployments"
import { FilesTab } from "@/routes/service/files"
import { NetworkingTab } from "@/routes/service/networking"
import { OverviewTab } from "@/routes/service/overview"
import { MetricsTab } from "@/routes/service/metrics"
import { PendingMark, SettingsTab } from "@/routes/service/settings-tab"
import { VariablesTab } from "@/routes/service/variables"
import { Events, Logs, ModeSwitch } from "@/routes/service/views"

const tabIcons: Record<Page, Icon> = {
  overview: InfoIcon,
  logs: TextAlignLeftIcon,
  deployments: ClockCounterClockwiseIcon,
  builds: HammerIcon,
  usage: ChartBarIcon,
  networking: GlobeIcon,
  variables: BracketsCurlyIcon,
  files: FolderIcon,
  settings: GearSixIcon,
}

/** The tab that holds each part a pending edit belongs to. */
const tabOfPart: Record<DraftPart, Page> = {
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
  const logs = useLogsReadable(project)
  const tab: Page = search.tab === "logs" && !logs ? "overview" : (search.tab ?? "overview")
  const mode = search.mode
  const fresh = useQuery(projectQuery(project))
  const go = (place: ServicePlace, pod?: string) =>
    void navigate(serviceLink(project, service, { tab: place, pod }))
  const setTab = (next: Page) =>
    void navigate({
      search: (previous) => ({
        ...previous,
        tab: next === "overview" ? undefined : next,
        mode: undefined,
        pod:
          next === "logs" || next === "deployments" || next === "files" ? previous.pod : undefined,
      }),
      hash: "",
    })
  const setMode = (next?: ServiceMode) =>
    void navigate({ search: (previous) => ({ ...previous, mode: next }), hash: "" })
  const setPod = (pod?: string) =>
    void navigate({ search: (previous) => ({ ...previous, pod: pod || undefined }), replace: true })
  const unreadable = data.unreadable
  const restartable =
    service.kind === "Deployment" || service.kind === "StatefulSet" || service.kind === "DaemonSet"
  const draft = useDraftState()
  const gates = useGates(project)
  const pending = usePresence<HTMLDivElement>(
    draft.count > 0 || draft.editing || draft.blocked !== undefined,
  )
  const reviewer = usePlanner(project, draft.applied, true)
  const unsaved = draft.count > 0
  // Tabs change only the search; leaving the service is what loses the edits.
  const leaving = useBlocker({
    shouldBlockFn: ({ current, next }) => unsaved && current.pathname !== next.pathname,
    enableBeforeUnload: () => unsaved,
    withResolver: true,
  })
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
  // Builds shows only for a service a Zeabur Builder Build makes an image of.
  const built = useServiceBuilds(project, service).built.length > 0
  const shown = serviceTabs.filter(
    (value) => (value !== "builds" || built || tab === "builds") && (value !== "logs" || logs),
  )
  const wide = tab === "logs"
  const pendingIn = new Set(draft.parts.map((part) => tabOfPart[part]))
  const kept = (value: Page) => editing.includes(value) && opened.includes(value)
  const copyName = () =>
    navigator.clipboard.writeText(id.toLowerCase()).then(
      () => toast.success(t("common.copied")),
      () => undefined,
    )
  const copyDiagnosis = async () => {
    try {
      const bundle = await queryClient.fetchQuery(
        evidenceQuery(project, service.kind, service.name),
      )
      await navigator.clipboard.writeText(
        writeBundle(bundle.markdown, (sentence) => writeSentence(t, sentence)),
      )
      toast.success(t("common.copied"))
    } catch {
      toast.error(t("diagnosis.copyFailed"))
    }
  }

  return (
    <DraftProvider draft={draft}>
      <Tabs
        value={tab}
        onValueChange={(value) => setTab(value as Page)}
        variant="page"
        className={cn("flex flex-1 flex-col", wide && "min-h-0")}
      >
        <header className="shrink-0">
          <PageColumn width="content">
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
                  {statusOf(service).unchecked ? (
                    <Tooltip content={t("status.uncheckedHint")}>
                      <Button variant="muted" size="icon-xs" aria-label={t("status.uncheckedHint")}>
                        <InfoIcon aria-hidden="true" />
                      </Button>
                    </Tooltip>
                  ) : null}
                </div>
                <div className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-0.5 text-xs text-secondary">
                  {service.created ? (
                    <span className="whitespace-nowrap">
                      {t("service.created", formatDate(service.created, locale))}
                    </span>
                  ) : null}
                  <Freshness
                    project={project}
                    updatedAt={fresh.dataUpdatedAt}
                    failed={fresh.isRefetchError}
                  />
                </div>
              </div>
              <div className="flex shrink-0 items-center gap-1.5 max-sm:w-full max-sm:pl-13">
                {restart ? (
                  <>
                    <GateTip gate={gates("changeServices")} className="max-sm:hidden">
                      <Button
                        variant="outline"
                        size="sm"
                        className="max-sm:hidden"
                        onClick={restart}
                      >
                        <ArrowClockwiseIcon aria-hidden="true" data-icon="inline-start" />
                        {t("ops.restart")}
                      </Button>
                    </GateTip>
                    <GateTip gate={gates("changeServices")} className="sm:hidden">
                      <Button
                        variant="outline"
                        size="icon-sm"
                        className="sm:hidden"
                        aria-label={t("ops.restart")}
                        onClick={restart}
                      >
                        <ArrowClockwiseIcon aria-hidden="true" />
                      </Button>
                    </GateTip>
                  </>
                ) : null}
                {isTask(service.kind) ? null : (
                  <>
                    <GateTip gate={gates("openTerminal")} className="max-sm:hidden">
                      <Button
                        variant="outline"
                        size="sm"
                        className="max-sm:hidden"
                        onClick={() => openShell(project, service, search.pod)}
                      >
                        <TerminalWindowIcon aria-hidden="true" data-icon="inline-start" />
                        {t("tab.terminal")}
                      </Button>
                    </GateTip>
                    <GateTip gate={gates("openTerminal")} className="sm:hidden">
                      <Button
                        variant="outline"
                        size="icon-sm"
                        className="sm:hidden"
                        aria-label={t("tab.terminal")}
                        onClick={() => openShell(project, service, search.pod)}
                      >
                        <TerminalWindowIcon aria-hidden="true" />
                      </Button>
                    </GateTip>
                  </>
                )}
                <Menu>
                  <MenuTrigger
                    render={
                      <Button variant="outline" size="icon-sm" aria-label={t("service.more")} />
                    }
                  >
                    <DotsThreeIcon aria-hidden="true" />
                  </MenuTrigger>
                  <MenuContent>
                    <MenuItem onClick={() => void copyDiagnosis()}>
                      <CopyIcon aria-hidden="true" />
                      {t("diagnosis.copy")}
                    </MenuItem>
                    <MenuItem onClick={() => void copyName()}>
                      <CopyIcon aria-hidden="true" />
                      {t("service.copyName")}
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
                      <GatedMenuItem gate={gates("deleteServices")} onClick={planDeletion}>
                        <TrashIcon aria-hidden="true" />
                        {t("service.delete")}
                      </GatedMenuItem>
                    )}
                  </MenuContent>
                </Menu>
              </div>
            </div>
            <RefusedSome unreadable={refusedElsewhere(unreadable, inServiceTabs)} />
            <TabRow>
              <TabsList aria-label={t("service.tabs")} className="w-full">
                {shown.map((value) => {
                  const TabIcon = tabIcons[value]
                  return (
                    <TabsTrigger key={value} value={value} className="flex-1">
                      <TabIcon aria-hidden="true" className="max-sm:hidden" />
                      <span className="relative">
                        {t(`tab.${value}`)}
                        {pendingIn.has(value) ? (
                          <PendingMark className="absolute top-1/2 left-full ml-1 -translate-y-1/2" />
                        ) : null}
                      </span>
                    </TabsTrigger>
                  )
                })}
              </TabsList>
            </TabRow>
          </PageColumn>
        </header>
        <PageColumn width="content" className={cn("mt-3", wide ? "mb-4 min-h-0 flex-1" : "mb-16")}>
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
              <div className="flex flex-col gap-6">
                <ModeSwitch tab="deployments" mode={mode} onMode={setMode} />
                {mode === "events" ? (
                  <Events events={mine} pod={search.pod} unreadable={unreadable} onPod={setPod} />
                ) : (
                  <DeploymentsTab
                    project={project}
                    service={service}
                    events={mine}
                    onEvents={() => go("events")}
                  />
                )}
              </div>
            ) : null}
          </TabsContent>
          <TabsContent value="builds">
            {tab === "builds" ? <BuildsTab project={project} service={service} /> : null}
          </TabsContent>
          <TabsContent value="usage">
            {tab === "usage" ? <MetricsTab project={project} service={service} /> : null}
          </TabsContent>
          <TabsContent value="networking" keepMounted={kept("networking")}>
            {tab === "networking" || kept("networking") ? (
              <div key={draft.generation} className="flex flex-col gap-6">
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
                <VariablesTab
                  project={project}
                  service={service}
                  edges={edges.filter((e) => e.from === serviceId(service))}
                  refused={refusedAmong(unreadable, refusedPlaces.config)}
                />
              </div>
            ) : null}
          </TabsContent>
          <TabsContent value="files">
            {tab === "files" ? (
              <FilesTab project={project} service={service} pod={search.pod} onPodChange={setPod} />
            ) : null}
          </TabsContent>
          <TabsContent value="settings" keepMounted={kept("settings")}>
            {tab === "settings" || kept("settings") ? (
              <SettingsTab
                project={project}
                service={service}
                network={data}
                unreadable={unreadable}
                mode={mode}
                onMode={setMode}
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
                  changes: draft.withHeld(),
                  notes: draft.notes.length > 0 ? draft.notes : undefined,
                  service: { kind: service.kind, name: service.name },
                })
              }
            />
          ) : null}
        </PageColumn>
        {planner.dialog}
        {deleter.dialog}
        {reviewer.dialog}
        {leaving.status === "blocked" ? (
          <LeaveDialog count={draft.count} onStay={leaving.reset} onLeave={leaving.proceed} />
        ) : null}
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
  const editing = draft.editing
  const [shown, setShown] = useState({ count: draft.count, detail, editing })
  if (
    (draft.count > 0 || editing || draft.blocked !== undefined) &&
    (draft.count !== shown.count || detail !== shown.detail || editing !== shown.editing)
  )
    setShown({ count: draft.count, detail, editing })
  // The wrapper sticks in the bar's place, so it can fade the bar in and out.
  return (
    <div {...presence} className="sticky bottom-3 z-10 mt-6 overlay-motion">
      <DraftBar
        label={shown.editing && shown.count === 0 ? t("draft.none") : t("draft.count", shown.count)}
        detail={shown.count > 0 ? shown.detail : undefined}
        blocked={draft.blocked}
        actions={
          <>
            <Button variant="ghost" size="sm" onClick={draft.discard}>
              {shown.editing && shown.count === 0 ? t("edit.cancel") : t("draft.discard")}
            </Button>
            {shown.count === 0 ? null : (
              <Button size="sm" onClick={onReview} disabled={draft.blocked !== undefined}>
                {shown.editing ? t("draft.save") : t("draft.review")}
              </Button>
            )}
          </>
        }
      />
    </div>
  )
}

function LeaveDialog({
  count,
  onStay,
  onLeave,
}: {
  count: number
  onStay: () => void
  onLeave: () => void
}) {
  const t = useT()
  return (
    <Dialog open onOpenChange={(open) => (open ? undefined : onStay())}>
      <DialogContent variant="compact">
        <div className="flex flex-col gap-2 px-5 pt-4 pb-3">
          <DialogTitle>{t("draft.leaveTitle")}</DialogTitle>
          <DialogDescription>{t("draft.leaveBody", count)}</DialogDescription>
        </div>
        <div className="flex justify-end gap-2 border-t px-5 py-3">
          <Button variant="ghost" size="sm" onClick={onStay}>
            {t("draft.stay")}
          </Button>
          <Button variant="destructive" size="sm" onClick={onLeave}>
            {t("draft.leave")}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}

function Column({ children }: { children: React.ReactNode }) {
  return (
    <PageColumn width="content" className="my-6 md:my-8">
      {children}
    </PageColumn>
  )
}

/** The tabs, with a button to scroll on while some of them are out of view. */
function TabRow({ children }: { children: ReactNode }) {
  const t = useT()
  const ref = useRef<HTMLDivElement>(null)
  const [overflow, setOverflow] = useState<"none" | "more" | "end">("none")
  const list = () => ref.current?.querySelector<HTMLElement>("[role=tablist]") ?? undefined
  const showing = overflow !== "none"
  useLayoutEffect(() => {
    const row = list()
    if (!row) return
    const check = () =>
      setOverflow(
        row.scrollWidth <= row.clientWidth + 1
          ? "none"
          : row.scrollLeft + row.clientWidth >= row.scrollWidth - 1
            ? "end"
            : "more",
      )
    check()
    row.addEventListener("scroll", check, { passive: true })
    const observer = new ResizeObserver(check)
    observer.observe(row)
    return () => {
      row.removeEventListener("scroll", check)
      observer.disconnect()
    }
  }, [])
  // The button narrows the row: keep the chosen tab clear of it.
  useLayoutEffect(() => {
    const row = list()
    const active = row?.querySelector<HTMLElement>("[aria-selected=true]")
    if (!row || !active || !showing) return
    const end = active.offsetLeft + active.offsetWidth - row.scrollLeft
    if (end > row.clientWidth) row.scrollLeft += end - row.clientWidth + 24
  }, [showing])
  return (
    <div ref={ref} className="flex items-center gap-1 pt-1 pb-3">
      <div className="min-w-0 flex-1">{children}</div>
      {overflow === "none" ? null : (
        <Button
          variant="muted"
          size="icon-sm"
          aria-label={t("service.moreTabs")}
          className={cn(overflow === "end" && "invisible")}
          onClick={() => {
            const row = list()
            row?.scrollBy({
              left: row.clientWidth * 0.6,
              behavior: matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth",
            })
          }}
        >
          <CaretRightIcon aria-hidden="true" />
        </Button>
      )}
    </div>
  )
}
