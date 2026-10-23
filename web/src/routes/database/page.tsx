// One database an operator runs, with a service's tabs: how it is and how to reach it, its
// members' logs, backups, events, and its settings with the operator's object as YAML.
import { useQuery } from "@tanstack/react-query"
import { getRouteApi, Link, useNavigate } from "@tanstack/react-router"
import { useState, type ReactNode } from "react"
import { toast } from "sonner"
import ArchiveIcon from "~icons/ph/archive"
import CaretRightIcon from "~icons/ph/caret-right"
import CopyIcon from "~icons/ph/copy"
import DatabaseIcon from "~icons/ph/database"
import DotsThreeIcon from "~icons/ph/dots-three"
import DownloadSimpleIcon from "~icons/ph/download-simple"
import EyeIcon from "~icons/ph/eye"
import EyeSlashIcon from "~icons/ph/eye-slash"
import GearSixIcon from "~icons/ph/gear-six"
import InfoIcon from "~icons/ph/info"
import ListBulletsIcon from "~icons/ph/list-bullets"
import PlugsConnectedIcon from "~icons/ph/plugs-connected"
import TerminalWindowIcon from "~icons/ph/terminal-window"
import TextAlignLeftIcon from "~icons/ph/text-align-left"
import TrashIcon from "~icons/ph/trash"
import WarningIcon from "~icons/ph/warning"

import { api, toProblem, unwrap, type Problem as ProblemBody, type Schemas } from "@/api/client"
import { useLogsReadable } from "@/api/logs-access"
import { databasesQuery, projectQuery } from "@/api/queries"
import { Ago } from "@/components/bits"
import { ConnectDrawer, useTargets } from "@/components/connect"
import { CopyText } from "@/components/copy-text"
import { DatabaseState } from "@/components/databases"
import { Diagnoses } from "@/components/diagnoses"
import { EventList } from "@/components/events"
import { FormField } from "@/components/form-field"
import { Freshness } from "@/components/freshness"
import { GatedMenuItem, GateTip, useGate, useGates } from "@/components/gate"
import { KubeDetails } from "@/components/kube-details"
import { usePlanner } from "@/components/plan/planner"
import { Problem } from "@/components/problem"
import { Schedule } from "@/components/schedule"
import { StatusPill } from "@/components/status"
import { useI18n, useT } from "@/i18n"
import { openDatabaseShell } from "@/lib/dock"
import type { Icon } from "@/lib/icon"
import { formatDate } from "@/lib/format"
import { cn } from "@/lib/utils"
import { partsOf } from "@/model/database-parts"
import {
  backUpNow,
  backupsOn,
  canBackUpNow,
  connectionUri,
  deleteDatabase,
  enableBackups,
  engineName,
  eventsOf,
  exportTool,
  exportUrl,
  fixedFolder,
  keepsEverything,
  lastRun,
  newBackupTarget,
  presetOf,
  serverContainer,
  stateHealth,
  targetProblems,
  type BackupTarget,
  type Database,
  type Frequency,
} from "@/model/databases"
import { connected } from "@/model/connect"
import { isWarning, type Event, type Health } from "@/model/service"
import { useProjectEvents } from "@/routes/project/data"
import { Restore, Restored } from "@/routes/database/restore"
import { DatabaseSettings } from "@/routes/database/settings"
import { databaseTabs, type DatabaseTab, type ServiceMode } from "@/routes/search"
import { TabRow } from "@/routes/service/tab-row"
import { Events, MemberLogs } from "@/routes/service/views"
import { Alert, AlertDescription } from "@zeabur/design-system/components/alert"
import { Button } from "@zeabur/design-system/components/button"
import { Container } from "@zeabur/design-system/components/container"
import { Input } from "@zeabur/design-system/components/input"
import {
  Menu,
  MenuContent,
  MenuItem,
  MenuSeparator,
  MenuTrigger,
} from "@zeabur/design-system/components/menu"
import { PageColumn } from "@zeabur/design-system/components/page-column"
import { Select } from "@zeabur/design-system/components/select"
import {
  SummaryCard,
  SummaryCardAside,
  SummaryCardFact,
  SummaryCardFacts,
  SummaryCardMain,
  SummaryCardStatus,
  SummaryCardWord,
} from "@zeabur/design-system/components/summary-card"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@zeabur/design-system/components/tabs"

const route = getRouteApi("/projects/$project/databases/$engine/$name")

const tabIcons: Record<DatabaseTab, Icon> = {
  overview: InfoIcon,
  logs: TextAlignLeftIcon,
  backups: ArchiveIcon,
  events: ListBulletsIcon,
  settings: GearSixIcon,
}

const tabLabel = {
  overview: "tab.overview",
  logs: "tab.logs",
  backups: "db.tab.backups",
  events: "tab.events",
  settings: "tab.settings",
} as const

const outcomeHealth: Record<Schemas["BackupOutcome"], Health> = {
  succeeded: "ok",
  partiallyFailed: "warn",
  failed: "fail",
  running: "progress",
  unknown: "idle",
}

export function DatabasePage() {
  const t = useT()
  const { project, engine, name } = route.useParams()
  const query = useQuery(databasesQuery(project))
  if (query.error)
    return (
      <Column>
        <Problem error={query.error} what={t("db.title")} retry={() => void query.refetch()} />
      </Column>
    )
  if (!query.data) return null
  const database = query.data.items.find((d) => d.engine === engine && d.name === name)
  if (!database)
    return (
      <Column>
        <p className="text-sm text-muted-foreground">{t("db.notFound")}</p>
        <Link to="/projects/$project" params={{ project }} className="text-sm hover:underline">
          {t("projectSettings.back")}
        </Link>
      </Column>
    )
  const operator = query.data.operators.find((o) => o.engine === database.engine)
  return (
    <Body
      project={project}
      database={database}
      backupsReadable={operator?.backups === "installed"}
      updatedAt={query.dataUpdatedAt}
      failed={query.isRefetchError}
    />
  )
}

function Column({ children }: { children: ReactNode }) {
  return (
    <PageColumn width="content" className="my-6 md:my-8">
      {children}
    </PageColumn>
  )
}

function Body({
  project,
  database,
  backupsReadable,
  updatedAt,
  failed,
}: {
  project: string
  database: Database
  backupsReadable: boolean
  updatedAt: number
  failed: boolean
}) {
  const t = useT()
  const { locale } = useI18n()
  const search = route.useSearch()
  const navigate = route.useNavigate()
  const leave = useNavigate()
  const logs = useLogsReadable(project)
  const gates = useGates(project)
  const events = eventsOf(useProjectEvents(project), database)
  const unreadable = useQuery(projectQuery(project)).data?.unreadable ?? []
  const tab: DatabaseTab = search.tab === "logs" && !logs ? "overview" : (search.tab ?? "overview")
  const shown = databaseTabs.filter((value) => value !== "logs" || logs)
  const wide = tab === "logs"
  const setTab = (next: DatabaseTab) =>
    void navigate({
      search: (previous) => ({
        tab: next === "overview" ? undefined : next,
        pod: next === "logs" || next === "events" ? previous.pod : undefined,
      }),
      hash: "",
    })
  const setMode = (next?: ServiceMode) =>
    void navigate({
      search: (previous) => ({ ...previous, mode: next === "yaml" ? "yaml" : undefined }),
      hash: "",
    })
  const setPod = (pod?: string) =>
    void navigate({ search: (previous) => ({ ...previous, pod: pod || undefined }), replace: true })
  const deleter = usePlanner(
    project,
    () => void leave({ to: "/projects/$project", params: { project }, search: {} }),
  )
  const shell = (pod?: string) => openDatabaseShell(project, database, pod)
  const copyName = () =>
    navigator.clipboard.writeText(`${database.resource}/${database.name}`).then(
      () => toast.success(t("common.copied")),
      () => undefined,
    )
  const terminalGate = gates("openTerminal")
  const tool = exportTool[database.engine]
  // MOCO's dump reads moco-writable's password from its Secret.
  const exportGate =
    terminalGate.open && database.engine === "mysql" ? gates("readSecrets") : terminalGate

  return (
    <Tabs
      value={tab}
      onValueChange={(value) => setTab(value as DatabaseTab)}
      variant="page"
      className={cn("flex flex-1 flex-col", wide && "min-h-0")}
    >
      <header className="shrink-0">
        <PageColumn width="content">
          <div className="flex flex-wrap items-start gap-x-3 gap-y-3 py-4 md:py-6">
            <span className="flex size-10 shrink-0 items-center justify-center rounded-xl border border-main bg-card">
              <DatabaseIcon aria-hidden="true" className="size-5" />
            </span>
            <div className="flex min-w-0 flex-1 flex-col gap-1">
              <div className="flex min-w-0 flex-wrap items-center gap-x-2.5 gap-y-1">
                <h1 className="min-w-0 truncate type-name" translate="no">
                  {database.name}
                </h1>
                <DatabaseState database={database} size="md" />
              </div>
              <div className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-0.5 text-xs text-secondary">
                <span className="whitespace-nowrap">
                  {engineName(database.engine)}
                  {database.version ? ` ${database.version}` : ""}
                </span>
                {database.created ? (
                  <span className="whitespace-nowrap">
                    {t("service.created", formatDate(database.created, locale))}
                  </span>
                ) : null}
                <Freshness project={project} updatedAt={updatedAt} failed={failed} />
              </div>
            </div>
            <div className="flex shrink-0 items-center gap-1.5 max-sm:w-full max-sm:pl-13">
              {backupsOn(database) && canBackUpNow(database.engine) ? (
                <BackUpNow project={project} database={database} />
              ) : null}
              <GateTip gate={terminalGate} className="max-sm:hidden">
                <Button
                  variant="outline"
                  size="sm"
                  className="max-sm:hidden"
                  onClick={() => shell(search.pod)}
                >
                  <TerminalWindowIcon aria-hidden="true" data-icon="inline-start" />
                  {t("tab.terminal")}
                </Button>
              </GateTip>
              <GateTip gate={terminalGate} className="sm:hidden">
                <Button
                  variant="outline"
                  size="icon-sm"
                  className="sm:hidden"
                  aria-label={t("tab.terminal")}
                  onClick={() => shell(search.pod)}
                >
                  <TerminalWindowIcon aria-hidden="true" />
                </Button>
              </GateTip>
              <Menu>
                <MenuTrigger
                  render={
                    <Button variant="outline" size="icon-sm" aria-label={t("service.more")} />
                  }
                >
                  <DotsThreeIcon aria-hidden="true" />
                </MenuTrigger>
                <MenuContent align="end" className="w-72">
                  <MenuItem onClick={() => void copyName()}>
                    <CopyIcon aria-hidden="true" />
                    {t("service.copyName")}
                  </MenuItem>
                  {tool ? (
                    <GatedMenuItem
                      gate={exportGate}
                      render={<a href={exportUrl(project, database)} download />}
                    >
                      <DownloadSimpleIcon aria-hidden="true" />
                      <span className="flex min-w-0 flex-col">
                        {t("db.export")}
                        <span className="type-meta text-muted-foreground">
                          {t("db.exportHint", tool)}
                        </span>
                      </span>
                    </GatedMenuItem>
                  ) : null}
                  <MenuSeparator />
                  <GatedMenuItem
                    gate={gates("changeDatabases")}
                    onClick={() =>
                      deleter.plan({
                        title: t("title.delete", database.kind, database.name),
                        changes: deleteDatabase(database),
                      })
                    }
                  >
                    <TrashIcon aria-hidden="true" />
                    {t("db.delete")}
                  </GatedMenuItem>
                </MenuContent>
              </Menu>
            </div>
          </div>
          <TabRow>
            <TabsList aria-label={t("db.tabs")} className="w-full">
              {shown.map((value) => {
                const TabIcon = tabIcons[value]
                return (
                  <TabsTrigger key={value} value={value} className="flex-1">
                    <TabIcon aria-hidden="true" className="max-sm:hidden" />
                    {t(tabLabel[value])}
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
            <Overview
              project={project}
              database={database}
              events={events}
              onEvents={() => setTab("events")}
            />
          ) : null}
        </TabsContent>
        <TabsContent value="logs" className={tab === "logs" ? "flex min-h-0 flex-col" : undefined}>
          {tab === "logs" ? (
            <MemberLogs
              project={project}
              source={{
                name: database.name,
                pods: database.pods,
                containers: [serverContainer[database.engine]],
                shell,
              }}
              pod={search.pod}
              onPodChange={setPod}
            />
          ) : null}
        </TabsContent>
        <TabsContent value="backups">
          {tab === "backups" ? (
            <Backups project={project} database={database} readable={backupsReadable} />
          ) : null}
        </TabsContent>
        <TabsContent value="events">
          {tab === "events" ? (
            <Events events={events} pod={search.pod} unreadable={unreadable} onPod={setPod} />
          ) : null}
        </TabsContent>
        <TabsContent value="settings">
          {tab === "settings" ? (
            <DatabaseSettings
              project={project}
              database={database}
              mode={search.mode}
              onMode={setMode}
            />
          ) : null}
        </TabsContent>
      </PageColumn>
      {deleter.dialog}
    </Tabs>
  )
}

function BackUpNow({ project, database }: { project: string; database: Database }) {
  const t = useT()
  const planner = usePlanner(project)
  const gate = useGate(project, "backUpDatabases")
  const starting = !database.backups.ready
  const plan = () =>
    planner.plan({
      title: t("db.backup.planNow", database.name),
      changes: backUpNow(database, new Date()),
    })
  return (
    <>
      {starting ? (
        <span id="db-backups-starting" className="type-meta text-muted-foreground max-sm:hidden">
          {t("db.backup.starting")}
        </span>
      ) : null}
      <GateTip gate={gate} className="max-sm:hidden">
        <Button
          variant="outline"
          size="sm"
          className="max-sm:hidden"
          disabled={starting}
          aria-describedby={starting ? "db-backups-starting" : undefined}
          onClick={plan}
        >
          <ArchiveIcon aria-hidden="true" data-icon="inline-start" />
          {t("db.backup.now")}
        </Button>
      </GateTip>
      <GateTip gate={gate} className="sm:hidden">
        <Button
          variant="outline"
          size="icon-sm"
          className="sm:hidden"
          disabled={starting}
          aria-label={t("db.backup.now")}
          title={starting ? t("db.backup.starting") : undefined}
          onClick={plan}
        >
          <ArchiveIcon aria-hidden="true" />
        </Button>
      </GateTip>
      {planner.dialog}
    </>
  )
}

/** The newest warnings, as a service's overview shows them, and the way to all events. */
function Warnings({ events, onEvents }: { events: Event[]; onEvents: () => void }) {
  const t = useT()
  const warnings = events.filter(isWarning)
  if (warnings.length === 0) return null
  return (
    <Container
      title={t("overview.warnings")}
      titleAction={
        <button
          type="button"
          onClick={onEvents}
          className="flex items-center gap-0.5 rounded text-xs font-medium text-secondary outline-none hover:text-main"
        >
          {t("common.all")}
          <CaretRightIcon className="size-3.5" aria-hidden="true" />
        </button>
      }
      variant="list"
    >
      <div className="px-4">
        <EventList events={warnings} limit={3} compact plain />
      </div>
    </Container>
  )
}

function Section({ id, title, children }: { id: string; title: string; children: ReactNode }) {
  return (
    <section aria-labelledby={id} className="flex flex-col gap-2">
      <h2 id={id} className="type-heading">
        {title}
      </h2>
      {children}
    </section>
  )
}

function Overview({
  project,
  database,
  events,
  onEvents,
}: {
  project: string
  database: Database
  events: Event[]
  onEvents: () => void
}) {
  const t = useT()
  const preset = presetOf(database.engine, database.resources)
  const run = lastRun(database)
  const services = useQuery(projectQuery(project)).data?.services ?? []
  const troubled = partsOf(database, services).find((s) => (s.diagnoses?.length ?? 0) > 0)
  return (
    <div className="flex flex-col gap-7">
      <Restored project={project} database={database} />
      <SummaryCard>
        <SummaryCardMain>
          <SummaryCardStatus tone={stateHealth[database.state]}>
            <SummaryCardWord>{t(`db.state.${database.state}`)}</SummaryCardWord>
          </SummaryCardStatus>
          <SummaryCardFacts>
            <SummaryCardFact label={t("db.fact.members")}>
              {t("db.membersReady", database.ready, database.members)}
            </SummaryCardFact>
            {database.primary ? (
              <SummaryCardFact label={t("db.fact.primary")}>
                <span className="truncate type-mono" translate="no">
                  {database.primary}
                </span>
              </SummaryCardFact>
            ) : null}
            <SummaryCardFact label={t("db.fact.version")}>
              <span className="truncate" translate="no" title={database.image ?? undefined}>
                {database.version ?? "—"}
              </span>
            </SummaryCardFact>
            <SummaryCardFact label={t("db.fact.storage")}>
              <span translate="no">
                {database.storage
                  ? database.members > 1
                    ? `${database.members} × ${database.storage}`
                    : database.storage
                  : "—"}
              </span>
            </SummaryCardFact>
            <SummaryCardFact label={t("db.fact.size")}>
              {preset ? t(`db.size.${preset}`) : t("db.fact.custom")}
            </SummaryCardFact>
            <SummaryCardFact label={t("db.backup.last")}>
              {run ? (
                <>
                  <StatusPill health={outcomeHealth[run.outcome]} size="sm">
                    {t(`backups.${run.outcome}`)}
                  </StatusPill>
                  <Ago iso={run.finished ?? run.started} />
                </>
              ) : (
                <span className="text-muted-foreground">
                  {backupsOn(database) ? t("db.backup.noRun") : t("db.backup.none")}
                </span>
              )}
            </SummaryCardFact>
          </SummaryCardFacts>
        </SummaryCardMain>
        {database.message && database.state !== "ready" ? (
          <SummaryCardAside>
            <p className="type-meta text-muted-foreground">{t("db.fact.operator")}</p>
            <p className="text-sm wrap-anywhere">{database.message}</p>
          </SummaryCardAside>
        ) : null}
      </SummaryCard>
      {database.members === 1 ? (
        <Alert variant="warn">
          <WarningIcon aria-hidden="true" />
          <AlertDescription>
            {t("db.noRedundancy", Boolean(database.backups.store?.inUse))}
          </AlertDescription>
        </Alert>
      ) : null}
      {troubled ? (
        <Diagnoses diagnoses={troubled.diagnoses} service={troubled} project={project} />
      ) : null}
      {database.engine === "dragonfly" && !database.app ? (
        <Alert variant="warn">
          <WarningIcon aria-hidden="true" />
          <AlertDescription>{t("db.noAuth", engineName(database.engine))}</AlertDescription>
        </Alert>
      ) : null}
      <KubeDetails
        facts={[
          [
            t("db.fact.members"),
            `${database.kind}/${database.name} ${membersField[database.engine]}`,
          ],
        ]}
      />
      <Warnings events={events} onEvents={onEvents} />
      <Connection project={project} database={database} />
    </div>
  )
}

/** Where each operator's object counts its members. */
const membersField: Record<Database["engine"], string> = {
  postgresql: "spec.instances",
  mongodb: "spec.replsets[].size",
  mysql: "spec.replicas",
  mariadb: "spec.replicas",
  dragonfly: "spec.replicas",
}

/** Rows in a card, labels and values at the summary card's facts' x. */
function Rows({ children }: { children: ReactNode }) {
  return (
    <dl className="flex flex-col divide-y divide-main rounded-lg border border-main bg-card">
      {children}
    </dl>
  )
}

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex min-w-0 items-center gap-x-4 px-4 py-1.5">
      <dt className="w-fact-label shrink-0 type-meta text-muted-foreground">{label}</dt>
      <dd className="flex min-h-8 min-w-0 flex-1 items-center gap-2">{children}</dd>
    </div>
  )
}

function Connection({ project, database }: { project: string; database: Database }) {
  const t = useT()
  const c = database.connection
  const revealGate = useGate(project, "readSecrets")
  const [secret, setSecret] = useState<{
    pending?: boolean
    values?: Schemas["RevealedSecret"]["values"]
    problem?: ProblemBody
  }>()
  const reveal = async () => {
    setSecret({ pending: true })
    try {
      const answer = unwrap(
        await api.POST("/api/v1/projects/{project}/secrets/{name}/reveal", {
          params: { path: { project, name: c.secret } },
          cache: "no-store",
        }),
      )
      setSecret({ values: answer.values })
    } catch (error) {
      setSecret({ problem: toProblem(error) })
    }
  }
  const password = secret?.values?.[c.passwordKey]?.value
  const username =
    c.username ?? (c.usernameKey ? secret?.values?.[c.usernameKey]?.value : undefined)
  const shown = password !== undefined
  const uri = connectionUri({ ...database, connection: { ...c, username } }, password)
  const mono = (text: string) => (
    <span className="min-w-0 truncate type-mono" translate="no">
      {text}
    </span>
  )
  return (
    <Section id="db-connect" title={t("db.connect")}>
      <ConnectServices project={project} database={database} />
      <p className="type-meta text-muted-foreground">{t("db.inCluster")}</p>
      {database.engine === "mysql" ? (
        <p className="type-meta text-muted-foreground">{t("db.noDatabase")}</p>
      ) : null}
      <Rows>
        <Row label={t("db.host")}>
          {mono(c.host)}
          <CopyText value={() => c.host} label={t("common.copy")} />
        </Row>
        <Row label={t("db.port")}>{mono(String(c.port))}</Row>
        {c.database ? <Row label={t("db.database")}>{mono(c.database)}</Row> : null}
        {c.username || c.usernameKey ? (
          <Row label={t("db.username")}>
            {username ? (
              mono(username)
            ) : (
              <span className="type-meta text-muted-foreground">{t("db.inSecret", c.secret)}</span>
            )}
          </Row>
        ) : null}
        <Row label={t("db.password")}>
          {shown ? mono(password) : mono("••••••••")}
          {shown ? (
            <>
              <CopyText value={() => password} label={t("common.copy")} />
              <Button variant="ghost" size="xs" onClick={() => setSecret(undefined)}>
                <EyeSlashIcon aria-hidden="true" data-icon="inline-start" />
                {t("db.hide")}
              </Button>
            </>
          ) : (
            <GateTip gate={revealGate}>
              <Button
                variant="ghost"
                size="xs"
                loading={secret?.pending}
                onClick={() => void reveal()}
              >
                <EyeIcon aria-hidden="true" data-icon="inline-start" />
                {t("db.reveal")}
              </Button>
            </GateTip>
          )}
        </Row>
        <Row label={t("db.uri")}>
          {mono(uri)}
          <CopyText value={() => uri} label={t("common.copy")} />
        </Row>
      </Rows>
      {secret?.problem ? <Problem problem={secret.problem} what={c.secret} /> : null}
    </Section>
  )
}

/** Which services read the database, and connecting another. */
function ConnectServices({ project, database }: { project: string; database: Database }) {
  const t = useT()
  const [open, setOpen] = useState(false)
  const services = useQuery(projectQuery(project)).data?.services ?? []
  const targets = useTargets(project)
  const connectGate = useGate(project, "changeServices")
  const readers = services.filter((s) => connected(s, database)).map((s) => s.name)
  if (!database.app)
    return <p className="type-meta text-muted-foreground">{t("connect.noApp", database.name)}</p>
  return (
    <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
      <p className="min-w-0 flex-1 text-sm">
        {readers.length > 0 ? t("connect.readBy", t("common.list", readers)) : t("connect.none")}
      </p>
      {targets.some((s) => !connected(s, database)) ? (
        <GateTip gate={connectGate}>
          <Button variant="outline" size="xs" onClick={() => setOpen(true)}>
            <PlugsConnectedIcon aria-hidden="true" data-icon="inline-start" />
            {t("connect.action")}
          </Button>
        </GateTip>
      ) : null}
      {open ? (
        <ConnectDrawer project={project} database={database} onClose={() => setOpen(false)} />
      ) : null}
    </div>
  )
}

function Backups({
  project,
  database,
  readable,
}: {
  project: string
  database: Database
  readable: boolean
}) {
  const t = useT()
  const store = database.backups.store
  const on = backupsOn(database)
  const pluginMissing = database.engine === "postgresql" && !readable
  return (
    <div className="flex flex-col gap-7">
      {pluginMissing ? (
        <Alert variant="warn">
          <WarningIcon aria-hidden="true" />
          <AlertDescription>
            {t("db.backup.pluginMissing")}{" "}
            <Link to="/cluster/components" className="underline">
              {t("backups.seeComponents")}
            </Link>
          </AlertDescription>
        </Alert>
      ) : null}
      {on && store ? (
        <Target database={database} />
      ) : pluginMissing ? null : (
        <>
          {store ? (
            <p className="type-meta text-muted-foreground">
              {t("db.backup.storeIdle", store.name)}
            </p>
          ) : null}
          <SetUp project={project} database={database} />
        </>
      )}
      {database.engine === "dragonfly" ? null : <Runs database={database} />}
      <Section id="db-restore" title={t("db.restore.action")}>
        <Restore project={project} database={database} />
      </Section>
    </div>
  )
}

function Target({ database }: { database: Database }) {
  const t = useT()
  const store = database.backups.store
  if (!store) return null
  const days = store.retention?.match(/^(\d+)d$/)?.[1]
  const hours = Number(store.retention?.match(/^(\d+)h$/)?.[1] ?? Number.NaN)
  const retention = !store.retention
    ? keepsEverything(database.engine)
      ? t("db.backup.keepsEverything")
      : "—"
    : database.engine === "mongodb"
      ? t("db.backup.retentionCount", store.retention)
      : days
        ? t("db.backup.retentionDays", Number(days))
        : hours % 24 === 0
          ? t("db.backup.retentionDays", hours / 24)
          : store.retention
  return (
    <Section id="db-target" title={t("db.backup.destination")}>
      <Rows>
        <Row label={t("db.backup.destination")}>
          <span className="min-w-0 truncate type-mono" translate="no">
            {store.destination ?? store.name}
          </span>
        </Row>
        {store.endpoint ? (
          <Row label={t("db.backup.endpointFact")}>
            <span className="min-w-0 truncate type-mono" translate="no">
              {store.endpoint}
            </span>
          </Row>
        ) : null}
        <Row label={t("db.backup.schedule")}>
          {database.backups.schedules.length === 0 ? (
            <span className="text-muted-foreground">{t("db.backup.unscheduled")}</span>
          ) : (
            database.backups.schedules.map((s) => (
              <span key={s.name} className="flex items-center gap-2">
                <Schedule cron={s.cron} />
                {s.paused ? (
                  <StatusPill health="idle" size="sm" dot={false}>
                    {t("backups.paused")}
                  </StatusPill>
                ) : null}
              </span>
            ))
          )}
        </Row>
        <Row label={t("db.backup.retention")}>{retention}</Row>
      </Rows>
      {database.engine === "postgresql" ? (
        <p className="type-meta text-muted-foreground">{t("db.backup.walNote")}</p>
      ) : null}
      {database.engine === "dragonfly" ? (
        <p className="type-meta text-muted-foreground">{t("db.backup.snapshots")}</p>
      ) : null}
    </Section>
  )
}

function Runs({ database }: { database: Database }) {
  const t = useT()
  const runs = database.backups.runs
  return (
    <Section id="db-runs" title={t("db.backup.runs")}>
      {runs.length === 0 ? (
        <p className="type-meta text-muted-foreground">{t("db.backup.noRun")}</p>
      ) : (
        <ul className="flex flex-col">
          {runs.map((r) => (
            <li key={r.name} className="flex items-center gap-2.5 border-t border-rule py-2.5">
              <div className="flex min-w-0 flex-1 flex-col">
                <span className="truncate type-mono" translate="no">
                  {r.name}
                </span>
                {r.message ? (
                  <span className="truncate type-meta text-muted-foreground" title={r.message}>
                    {r.message}
                  </span>
                ) : null}
              </div>
              <StatusPill health={outcomeHealth[r.outcome]} size="sm">
                {t(`backups.${r.outcome}`)}
              </StatusPill>
              <Ago
                iso={r.finished ?? r.started}
                className="w-20 shrink-0 text-right type-meta text-muted-foreground"
              />
            </li>
          ))}
        </ul>
      )}
    </Section>
  )
}

function SetUp({ project, database }: { project: string; database: Database }) {
  const t = useT()
  const [target, setTarget] = useState<BackupTarget>(newBackupTarget)
  const [tried, setTried] = useState(false)
  const planner = usePlanner(project)
  const setUpGate = useGate(project, "changeDatabases")
  const problems = targetProblems(target)
  const set = (patch: Partial<BackupTarget>) => setTarget((v) => ({ ...v, ...patch }))
  const error = (problem: (typeof problems)[number]) =>
    tried && problems.includes(problem) ? [t(`db.backup.problem.${problem}`)] : []
  const review = () => {
    setTried(true)
    if (problems.length > 0) return
    planner.plan({
      title: t("db.backup.plan", database.name),
      changes: enableBackups(database, target),
      notes:
        database.engine === "dragonfly" && database.members === 1
          ? [t("db.backup.startsEmpty")]
          : undefined,
    })
  }
  return (
    <Section id="db-setup" title={t("db.backup.setUp")}>
      <p className="text-sm text-muted-foreground">{t("db.backup.setUpBody")}</p>
      <form
        className="flex flex-col gap-4 rounded-xl border border-main bg-card p-5"
        onSubmit={(event) => {
          event.preventDefault()
          review()
        }}
      >
        <FormField label={t("db.backup.endpoint")} errors={error("endpoint")}>
          <Input
            size="sm"
            mono
            placeholder="https://s3.example.com"
            value={target.endpoint}
            autoComplete="off"
            spellCheck={false}
            onChange={(event) => set({ endpoint: event.target.value })}
          />
        </FormField>
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
          <FormField label={t("db.backup.bucket")} errors={error("bucket")}>
            <Input
              size="sm"
              mono
              value={target.bucket}
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => set({ bucket: event.target.value })}
            />
          </FormField>
          {fixedFolder(database.engine) ? null : (
            <FormField label={t("db.backup.path")} hint={t("db.backup.pathHint")}>
              <Input
                size="sm"
                mono
                placeholder={database.name}
                value={target.path}
                autoComplete="off"
                spellCheck={false}
                onChange={(event) => set({ path: event.target.value })}
              />
            </FormField>
          )}
          <FormField label={t("db.backup.accessKey")} errors={error("keys")}>
            <Input
              size="sm"
              mono
              value={target.accessKey}
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => set({ accessKey: event.target.value })}
            />
          </FormField>
          <FormField label={t("db.backup.secretKey")}>
            <Input
              size="sm"
              mono
              type="password"
              value={target.secretKey}
              autoComplete="new-password"
              onChange={(event) => set({ secretKey: event.target.value })}
            />
          </FormField>
          <FormField label={t("db.backup.region")} hint={t("db.backup.regionHint")}>
            <Input
              size="sm"
              mono
              value={target.region}
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => set({ region: event.target.value })}
            />
          </FormField>
          <FormField label={t("db.backup.frequency")}>
            <Select
              size="sm"
              value={target.frequency}
              onValueChange={(frequency) => set({ frequency: frequency as Frequency })}
              items={[
                { value: "daily", label: t("db.backup.daily") },
                { value: "weekly", label: t("db.backup.weekly") },
              ]}
            />
          </FormField>
          {keepsEverything(database.engine) ? null : (
            <FormField label={t("db.backup.keep")} errors={error("keep")}>
              <Input
                size="sm"
                type="number"
                min={1}
                max={365}
                value={String(target.keepDays)}
                onChange={(event) => set({ keepDays: Number(event.target.value) })}
              />
            </FormField>
          )}
        </div>
        {fixedFolder(database.engine) ? (
          <p className="type-meta text-muted-foreground">
            {t("db.backup.fixedFolder", `moco/${project}/${database.name}`)}
          </p>
        ) : null}
        {keepsEverything(database.engine) ? (
          <p className="type-meta text-muted-foreground">{t("db.backup.keepsEverything")}</p>
        ) : null}
        {database.engine === "dragonfly" && database.members === 1 ? (
          <Alert variant="warn">
            <WarningIcon aria-hidden="true" />
            <AlertDescription>{t("db.backup.startsEmpty")}</AlertDescription>
          </Alert>
        ) : null}
        <div className="flex justify-end">
          <GateTip gate={setUpGate}>
            <Button size="sm" type="submit">
              {t("edit.review")}
            </Button>
          </GateTip>
        </div>
      </form>
      {planner.dialog}
    </Section>
  )
}
