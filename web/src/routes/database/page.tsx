// One database an operator runs, with a service's tabs: how it is and how to reach it, its
// members' logs, backups, events, and its settings with the operator's object as YAML.
import { useQuery } from "@tanstack/react-query"
import { getRouteApi, Link } from "@tanstack/react-router"
import { useState, type ReactNode } from "react"
import ArchiveIcon from "~icons/ph/archive"
import DatabaseIcon from "~icons/ph/database"
import DownloadSimpleIcon from "~icons/ph/download-simple"
import GearSixIcon from "~icons/ph/gear-six"
import InfoIcon from "~icons/ph/info"
import ListBulletsIcon from "~icons/ph/list-bullets"
import TerminalWindowIcon from "~icons/ph/terminal-window"
import TextAlignLeftIcon from "~icons/ph/text-align-left"
import WarningIcon from "~icons/ph/warning"

import type { Schemas } from "@/api/client"
import { databasesQuery, projectQuery } from "@/api/queries"
import { Ago } from "@/components/bits"
import { ConnectServices } from "@/components/connect"
import { CopyText } from "@/components/copy-text"
import { useBackups } from "@/components/backups"
import { DatabaseState } from "@/components/databases"
import {
  CardRow,
  CardRows,
  RevealRow,
  Section,
  useReveal,
  Warnings,
} from "@/components/store-detail"
import { Diagnoses } from "@/components/diagnoses"
import { FormField } from "@/components/form-field"
import { GatedMenuItem, GateTip, useGate } from "@/components/gate"
import { KubeDetails } from "@/components/kube-details"
import { usePlanner } from "@/components/plan/planner"
import { Problem } from "@/components/problem"
import { FrequencyField, S3Fields } from "@/components/s3-fields"
import { Schedule } from "@/components/schedule"
import { StatusPill } from "@/components/status"
import { useT } from "@/i18n"
import { openDatabaseShell } from "@/lib/dock"
import type { Icon } from "@/lib/icon"
import { partsOf } from "@/model/database-parts"
import {
  APP_DATABASE,
  backUpNow,
  backupsOn,
  canBackUpNow,
  connectionUri,
  deleteDatabase,
  backupsSummary,
  backUpNowSummary,
  enableBackups,
  engineName,
  eventsOf,
  backupNames,
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
} from "@/model/databases"
import type { Event, Health } from "@/model/service"
import { useProjectEvents } from "@/routes/project/data"
import { Restore, Restored } from "@/routes/database/restore"
import { DatabaseSettings } from "@/routes/database/settings"
import { databaseTabs, type DatabaseTab } from "@/routes/search"
import { StoreMenu, StoreShell, useStorePage } from "@/routes/store/shell"
import { Events, MemberLogs } from "@/routes/service/views"
import { Alert, AlertDescription } from "@zeabur/design-system/components/alert"
import { Button } from "@zeabur/design-system/components/button"
import { Input } from "@zeabur/design-system/components/input"
import { PageColumn } from "@zeabur/design-system/components/page-column"
import {
  SummaryCard,
  SummaryCardAside,
  SummaryCardFact,
  SummaryCardFacts,
  SummaryCardMain,
  SummaryCardStatus,
  SummaryCardWord,
} from "@zeabur/design-system/components/summary-card"
import { TabsContent } from "@zeabur/design-system/components/tabs"

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
  const search = route.useSearch()
  const { tab, shown, gates, unreadable, deleter, setTab, setPod, setMode } = useStorePage(
    project,
    search.tab,
    databaseTabs,
  )
  const events = eventsOf(useProjectEvents(project), database)
  const shell = (pod?: string) => openDatabaseShell(project, database, pod)
  const terminalGate = gates("openTerminal")

  return (
    <StoreShell
      project={project}
      icon={DatabaseIcon}
      name={database.name}
      state={<DatabaseState database={database} size="md" />}
      kind={`${engineName(database.engine)}${database.version ? ` ${database.version}` : ""}`}
      created={database.created}
      updatedAt={updatedAt}
      failed={failed}
      tab={tab}
      tabs={shown.map((value) => ({ value, icon: tabIcons[value], label: t(tabLabel[value]) }))}
      tabsLabel={t("db.tabs")}
      onTab={setTab}
      actions={
        <>
          {backupsOn(database) ? <BackUpNow project={project} database={database} /> : null}
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
          <StoreMenu
            copy={`${database.resource}/${database.name}`}
            deleteGate={gates("changeDatabases")}
            deleteLabel={t("db.delete")}
            onDelete={() =>
              deleter.plan({
                title: t("title.delete", database.kind, database.name),
                changes: deleteDatabase(database),
              })
            }
          >
            <ExportItem project={project} database={database} />
          </StoreMenu>
        </>
      }
    >
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
      {deleter.dialog}
    </StoreShell>
  )
}

/** A dump through the person's own exec into the database's pod. */
function ExportItem({ project, database }: { project: string; database: Database }) {
  const t = useT()
  const terminalGate = useGate(project, "openTerminal")
  const secretsGate = useGate(project, "readSecrets")
  const tool = exportTool[database.engine]
  if (!tool) return null
  // MOCO's dump reads moco-writable's password from its Secret.
  const gate = terminalGate.open && database.engine === "mysql" ? secretsGate : terminalGate
  return (
    <GatedMenuItem gate={gate} render={<a href={exportUrl(project, database)} download />}>
      <DownloadSimpleIcon aria-hidden="true" />
      <span className="flex min-w-0 flex-col">
        {t("db.export")}
        <span className="type-meta text-muted-foreground">{t("db.exportHint", tool)}</span>
      </span>
    </GatedMenuItem>
  )
}

function BackUpNow({ project, database }: { project: string; database: Database }) {
  const t = useT()
  const planner = usePlanner(project)
  const gate = useGate(project, "backUpDatabases")
  const starting = !database.backups.ready
  if (!canBackUpNow(database.engine)) return null
  const plan = () =>
    planner.plan({
      title: t("db.backup.planNow", database.name),
      changes: backUpNow(database, new Date()),
      summary: backUpNowSummary(t, database),
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
      <Cautions project={project} database={database} />
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

/** Warnings about this database and the services under it. */
function Cautions({ project, database }: { project: string; database: Database }) {
  const t = useT()
  const services = useQuery(projectQuery(project)).data?.services ?? []
  const troubled = partsOf(database, services).find((s) => (s.diagnoses?.length ?? 0) > 0)
  return (
    <>
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
    </>
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

function Connection({ project, database }: { project: string; database: Database }) {
  const t = useT()
  const c = database.connection
  const revealed = useReveal(project, c.secret)
  const secret = revealed.secret
  const password = secret?.values?.[c.passwordKey]?.value
  const username =
    c.username ?? (c.usernameKey ? secret?.values?.[c.usernameKey]?.value : undefined)
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
      <NoAppDatabase database={database} />
      <CardRows>
        <CardRow label={t("db.host")}>
          {mono(c.host)}
          <CopyText value={() => c.host} label={t("common.copy")} />
        </CardRow>
        <CardRow label={t("db.port")}>{mono(String(c.port))}</CardRow>
        {c.database ? <CardRow label={t("db.database")}>{mono(c.database)}</CardRow> : null}
        <UsernameRow connection={c}>
          {username ? mono(username) : <InSecret secret={c.secret} />}
        </UsernameRow>
        <RevealRow label={t("db.password")} value={password} reveal={revealed} />
        <CardRow label={t("db.uri")}>
          {mono(uri)}
          <CopyText value={() => uri} label={t("common.copy")} />
        </CardRow>
      </CardRows>
      {secret?.problem ? <Problem problem={secret.problem} what={c.secret} /> : null}
    </Section>
  )
}

/** A MOCO cluster made outside Zeabur has no app database. */
function NoAppDatabase({ database }: { database: Database }) {
  const t = useT()
  if (database.engine !== "mysql" || database.connection.database) return null
  return (
    <>
      <p className="type-meta text-muted-foreground">{t("db.noDatabase")}</p>
      <KubeDetails facts={[[t("db.appDatabase"), `metadata.annotations["${APP_DATABASE}"]`]]} />
    </>
  )
}

/** Dragonfly has no username to show. */
function UsernameRow({
  connection,
  children,
}: {
  connection: Database["connection"]
  children: ReactNode
}) {
  const t = useT()
  if (!connection.username && !connection.usernameKey) return null
  return <CardRow label={t("db.username")}>{children}</CardRow>
}

function InSecret({ secret }: { secret: string }) {
  const t = useT()
  return <span className="type-meta text-muted-foreground">{t("db.inSecret", secret)}</span>
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
            {t("db.backup.pluginMissing")}
            {t("common.gap")}
            <Link to="/cluster/components" className="underline">
              {t("backups.seeComponents")}
            </Link>
          </AlertDescription>
        </Alert>
      ) : null}
      {on && store ? (
        <Target project={project} database={database} />
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

const targetNote: Partial<Record<Database["engine"], "db.backup.walNote" | "db.backup.snapshots">> =
  { postgresql: "db.backup.walNote", dragonfly: "db.backup.snapshots" }

/** How long the destination keeps backups, as each operator counts it. */
function useRetention(database: Database) {
  const t = useT()
  const retention = database.backups.store?.retention
  if (!retention) return keepsEverything(database.engine) ? t("db.backup.keepsEverything") : "—"
  if (database.engine === "mongodb") return t("db.backup.retentionCount", retention)
  const days = daysOf(retention)
  return days === undefined ? retention : t("db.backup.retentionDays", days)
}

/** Whole days in a duration such as "7d" or "168h". */
function daysOf(duration: string) {
  const days = duration.match(/^(\d+)d$/)?.[1]
  if (days) return Number(days)
  const hours = Number(duration.match(/^(\d+)h$/)?.[1] ?? Number.NaN)
  return hours % 24 === 0 ? hours / 24 : undefined
}

function Target({ project, database }: { project: string; database: Database }) {
  const t = useT()
  const retention = useRetention(database)
  const store = database.backups.store
  if (!store) return null
  const note = targetNote[database.engine]
  return (
    <Section id="db-target" title={t("db.backup.destination")}>
      <CardRows>
        <CardRow label={t("db.backup.destination")}>
          <span className="min-w-0 truncate type-mono" translate="no">
            {store.destination ?? store.name}
          </span>
        </CardRow>
        {store.endpoint ? (
          <CardRow label={t("db.backup.endpointFact")}>
            <span className="min-w-0 truncate type-mono" translate="no">
              {store.endpoint}
            </span>
          </CardRow>
        ) : null}
        <CardRow label={t("db.backup.schedule")}>
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
        </CardRow>
        <CardRow label={t("db.backup.retention")}>{retention}</CardRow>
        <KeysFrom project={project} secret={store.secret ?? backupNames(database.name).secret} />
      </CardRows>
      {note ? <p className="type-meta text-muted-foreground">{t(note)}</p> : null}
    </Section>
  )
}

/** The S3 destination the keys were copied from, when they were. */
function KeysFrom({ project, secret }: { project: string; secret: string }) {
  const t = useT()
  const from = useBackups(project).data?.keysFrom[secret]
  return from ? (
    <CardRow label={t("clusterBackups.kube.keys")}>{t("s3d.from", from)}</CardRow>
  ) : null
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
  const startsEmpty = database.engine === "dragonfly" && database.members === 1
  const review = () => {
    setTried(true)
    if (problems.length > 0) return
    planner.plan({
      title: t("db.backup.plan", database.name),
      changes: enableBackups(database, target),
      summary: backupsSummary(t, database, target),
      notes: startsEmpty ? [t("db.backup.startsEmpty")] : undefined,
    })
  }
  const set = (patch: Partial<BackupTarget>) => setTarget((v) => ({ ...v, ...patch }))
  const error = (problem: (typeof problems)[number]) =>
    tried && problems.includes(problem) ? [t(`db.backup.problem.${problem}`)] : []
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
        <S3Fields
          target={target}
          set={set}
          errors={error}
          folder={fixedFolder(database.engine) ? undefined : database.name}
          folderHint={t("db.backup.pathHint")}
        >
          <FrequencyField value={target.frequency} onChange={(frequency) => set({ frequency })} />
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
        </S3Fields>
        {fixedFolder(database.engine) ? (
          <p className="type-meta text-muted-foreground">
            {t("db.backup.fixedFolder", `moco/${project}/${database.name}`)}
          </p>
        ) : null}
        {keepsEverything(database.engine) ? (
          <p className="type-meta text-muted-foreground">{t("db.backup.keepsEverything")}</p>
        ) : null}
        {startsEmpty ? (
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
