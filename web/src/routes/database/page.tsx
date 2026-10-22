// One database an operator runs: whether it serves and how to reach it, then its backups.
import { useQuery } from "@tanstack/react-query"
import { getRouteApi, Link } from "@tanstack/react-router"
import { useState, type ReactNode } from "react"
import ArchiveIcon from "~icons/ph/archive"
import DatabaseIcon from "~icons/ph/database"
import EyeIcon from "~icons/ph/eye"
import EyeSlashIcon from "~icons/ph/eye-slash"
import InfoIcon from "~icons/ph/info"
import WarningIcon from "~icons/ph/warning"

import { api, toProblem, unwrap, type Problem as ProblemBody, type Schemas } from "@/api/client"
import { databasesQuery } from "@/api/queries"
import { Ago } from "@/components/bits"
import { CopyText } from "@/components/copy-text"
import { DatabaseState } from "@/components/databases"
import { OpenInTerminal } from "@/components/dock/open-in-terminal"
import { FormField } from "@/components/form-field"
import { Freshness } from "@/components/freshness"
import { usePlanner } from "@/components/plan/planner"
import { Problem } from "@/components/problem"
import { StatusPill } from "@/components/status"
import { useI18n, useT } from "@/i18n"
import { formatDate } from "@/lib/format"
import {
  backUpNow,
  backupsOn,
  commands,
  connectionUri,
  enableBackups,
  engineName,
  lastRun,
  newBackupTarget,
  presetOf,
  stateHealth,
  targetProblems,
  type BackupTarget,
  type Database,
  type Frequency,
} from "@/model/databases"
import type { Health } from "@/model/service"
import { Alert, AlertDescription } from "@zeabur/design-system/components/alert"
import { Button } from "@zeabur/design-system/components/button"
import { Input } from "@zeabur/design-system/components/input"
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

type Tab = "overview" | "backups"

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
      <PageColumn width="content" className="mt-8">
        <Problem error={query.error} what={t("db.title")} retry={() => void query.refetch()} />
      </PageColumn>
    )
  if (!query.data) return null
  const database = query.data.items.find((d) => d.engine === engine && d.name === name)
  if (!database)
    return (
      <PageColumn width="content" className="mt-8">
        <p className="text-sm text-muted-foreground">{t("db.notFound")}</p>
        <Link to="/projects/$project" params={{ project }} className="text-sm hover:underline">
          {t("projectSettings.back")}
        </Link>
      </PageColumn>
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
  const tab: Tab = search.tab ?? "overview"
  const setTab = (next: Tab) =>
    void navigate({ search: { tab: next === "overview" ? undefined : next }, replace: true })
  return (
    <Tabs
      value={tab}
      onValueChange={(value) => setTab(value as Tab)}
      variant="page"
      className="flex flex-1 flex-col"
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
              {backupsOn(database) ? <BackUpNow project={project} database={database} /> : null}
            </div>
          </div>
          <TabsList aria-label={t("db.tabs")} className="w-full">
            <TabsTrigger value="overview" className="flex-1">
              <InfoIcon aria-hidden="true" className="max-sm:hidden" />
              {t("db.tab.overview")}
            </TabsTrigger>
            <TabsTrigger value="backups" className="flex-1">
              <ArchiveIcon aria-hidden="true" className="max-sm:hidden" />
              {t("db.tab.backups")}
            </TabsTrigger>
          </TabsList>
        </PageColumn>
      </header>
      <PageColumn width="content" gap="md" className="mt-3 mb-16">
        <TabsContent value="overview">
          {tab === "overview" ? <Overview project={project} database={database} /> : null}
        </TabsContent>
        <TabsContent value="backups">
          {tab === "backups" ? (
            <Backups project={project} database={database} readable={backupsReadable} />
          ) : null}
        </TabsContent>
      </PageColumn>
    </Tabs>
  )
}

function BackUpNow({ project, database }: { project: string; database: Database }) {
  const t = useT()
  const planner = usePlanner(project)
  const starting = !database.backups.ready
  return (
    <>
      {starting ? (
        <span id="db-backups-starting" className="type-meta text-muted-foreground">
          {t("db.backup.starting")}
        </span>
      ) : null}
      <Button
        variant="outline"
        size="sm"
        disabled={starting}
        aria-describedby={starting ? "db-backups-starting" : undefined}
        onClick={() =>
          planner.plan({
            title: t("db.backup.planNow", database.name),
            changes: backUpNow(database, new Date()),
          })
        }
      >
        <ArchiveIcon aria-hidden="true" data-icon="inline-start" />
        {t("db.backup.now")}
      </Button>
      {planner.dialog}
    </>
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

function Overview({ project, database }: { project: string; database: Database }) {
  const t = useT()
  const preset = presetOf(database.engine, database.resources)
  const run = lastRun(database)
  return (
    <div className="flex flex-col gap-7">
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
      <Connection project={project} database={database} />
      <Commands project={project} database={database} />
    </div>
  )
}

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-x-4 gap-y-0.5 border-t border-rule py-2.5 sm:flex-row sm:items-center">
      <dt className="shrink-0 type-meta text-muted-foreground sm:w-40">{label}</dt>
      <dd className="flex min-w-0 items-center gap-2">{children}</dd>
    </div>
  )
}

function Connection({ project, database }: { project: string; database: Database }) {
  const t = useT()
  const c = database.connection
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
      <p className="type-meta text-muted-foreground">{t("db.inCluster")}</p>
      <dl className="flex flex-col">
        <Row label={t("db.host")}>
          {mono(c.host)}
          <CopyText value={() => c.host} label={t("common.copy")} />
        </Row>
        <Row label={t("db.port")}>{mono(String(c.port))}</Row>
        {c.database ? <Row label={t("db.database")}>{mono(c.database)}</Row> : null}
        <Row label={t("db.username")}>
          {username ? (
            mono(username)
          ) : (
            <span className="type-meta text-muted-foreground">{t("db.inSecret", c.secret)}</span>
          )}
        </Row>
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
            <Button
              variant="ghost"
              size="xs"
              loading={secret?.pending}
              onClick={() => void reveal()}
            >
              <EyeIcon aria-hidden="true" data-icon="inline-start" />
              {t("db.reveal")}
            </Button>
          )}
        </Row>
        <Row label={t("db.uri")}>
          {mono(uri)}
          <CopyText value={() => uri} label={t("common.copy")} />
        </Row>
      </dl>
      {secret?.problem ? <Problem problem={secret.problem} what={c.secret} /> : null}
    </Section>
  )
}

function Commands({ project, database }: { project: string; database: Database }) {
  const t = useT()
  const { describe, shell } = commands(project, database)
  const rows = [
    { label: t("db.command.describe"), command: describe },
    { label: t("db.command.shell"), command: shell },
  ]
  return (
    <Section id="db-commands" title={t("db.commands")}>
      <ul className="flex flex-col gap-3">
        {rows.map((row) => (
          <li key={row.label} className="flex flex-col gap-1.5">
            <span className="type-meta text-muted-foreground">{row.label}</span>
            <div className="flex flex-wrap items-center gap-x-2 gap-y-1 rounded-lg border bg-muted/40 py-1.5 pr-1.5 pl-3">
              <code className="min-w-48 flex-1 type-mono break-all" translate="no">
                {row.command}
              </code>
              <div className="ml-auto flex items-center">
                <CopyText value={() => row.command} label={t("common.copy")} />
                <OpenInTerminal project={project} command={row.command} />
              </div>
            </div>
          </li>
        ))}
      </ul>
    </Section>
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
      <Runs database={database} />
      <p className="type-meta text-muted-foreground">{t("db.backup.restoreNext")}</p>
    </div>
  )
}

function Target({ database }: { database: Database }) {
  const t = useT()
  const store = database.backups.store
  if (!store) return null
  const days = store.retention?.match(/^(\d+)d$/)?.[1]
  const retention = !store.retention
    ? "—"
    : database.engine === "mongodb"
      ? t("db.backup.retentionCount", store.retention)
      : days
        ? t("db.backup.retentionDays", Number(days))
        : store.retention
  return (
    <Section id="db-target" title={t("db.backup.destination")}>
      <dl className="flex flex-col">
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
                <code className="type-mono" translate="no">
                  {s.cron}
                </code>
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
      </dl>
      {database.engine === "postgresql" ? (
        <p className="type-meta text-muted-foreground">{t("db.backup.walNote")}</p>
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
        </div>
        <div className="flex justify-end">
          <Button size="sm" type="submit">
            {t("edit.review")}
          </Button>
        </div>
      </form>
      {planner.dialog}
    </Section>
  )
}
