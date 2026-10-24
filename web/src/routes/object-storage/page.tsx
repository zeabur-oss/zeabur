// One object store the SeaweedFS Operator runs, with a service's tabs: how it is, where to
// connect and its buckets, its pods' logs, events, and the operator's object as YAML.
import { useQuery } from "@tanstack/react-query"
import { getRouteApi, Link } from "@tanstack/react-router"
import { useState, type ReactNode } from "react"
import GearSixIcon from "~icons/ph/gear-six"
import HardDrivesIcon from "~icons/ph/hard-drives"
import InfoIcon from "~icons/ph/info"
import ListBulletsIcon from "~icons/ph/list-bullets"
import PlusIcon from "~icons/ph/plus"
import TextAlignLeftIcon from "~icons/ph/text-align-left"
import TrashIcon from "~icons/ph/trash"
import WarningIcon from "~icons/ph/warning"

import { objectStorageQuery } from "@/api/queries"
import { ConnectServices } from "@/components/connect"
import { CopyText } from "@/components/copy-text"
import { NameField } from "@/components/form-field"
import { GateTip, useGate } from "@/components/gate"
import { KubeDetails } from "@/components/kube-details"
import { ObjectStorageState } from "@/components/object-storage"
import { PlanDrawer, ReviewFooter, ReviewForm } from "@/components/plan/plan-drawer"
import { usePlanner, usePlanStep } from "@/components/plan/planner"
import { Problem } from "@/components/problem"
import { StatusPill } from "@/components/status"
import {
  CardRow,
  CardRows,
  Mono,
  RevealRow,
  Section,
  useReveal,
  Warnings,
} from "@/components/store-detail"
import { useT } from "@/i18n"
import type { Icon } from "@/lib/icon"
import { useQuantityFormat } from "@/lib/quantity"
import { stateHealth } from "@/model/databases"
import {
  bucketProblem,
  connectable,
  createBucket,
  deleteBucket,
  deleteObjectStorage,
  storeEvents,
  type Bucket,
  type ObjectStorage,
} from "@/model/object-storage"
import type { Event } from "@/model/service"
import { useProjectEvents } from "@/routes/project/data"
import { ObjectStorageSettings } from "@/routes/object-storage/settings"
import { objectStorageTabs, type ObjectStorageTab } from "@/routes/search"
import { StoreMenu, StoreShell, useStorePage } from "@/routes/store/shell"
import { Events, MemberLogs } from "@/routes/service/views"
import { Alert, AlertDescription } from "@zeabur/design-system/components/alert"
import { Button } from "@zeabur/design-system/components/button"
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

const route = getRouteApi("/projects/$project/object-storage/$name")

const tabIcons: Record<ObjectStorageTab, Icon> = {
  overview: InfoIcon,
  logs: TextAlignLeftIcon,
  events: ListBulletsIcon,
  settings: GearSixIcon,
}

const tabLabel = {
  overview: "tab.overview",
  logs: "tab.logs",
  events: "tab.events",
  settings: "tab.settings",
} as const

const REGION = "us-east-1"

export function ObjectStoragePage() {
  const t = useT()
  const { project, name } = route.useParams()
  const query = useQuery(objectStorageQuery(project))
  if (query.error)
    return (
      <Column>
        <Problem error={query.error} what={t("s3.title")} retry={() => void query.refetch()} />
      </Column>
    )
  if (!query.data) return null
  const store = query.data.items.find((s) => s.name === name)
  if (!store)
    return (
      <Column>
        <p className="text-sm text-muted-foreground">{t("s3.notFound")}</p>
        <Link to="/projects/$project" params={{ project }} className="text-sm hover:underline">
          {t("projectSettings.back")}
        </Link>
      </Column>
    )
  return (
    <Body
      project={project}
      store={store}
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
  store,
  updatedAt,
  failed,
}: {
  project: string
  store: ObjectStorage
  updatedAt: number
  failed: boolean
}) {
  const t = useT()
  const search = route.useSearch()
  const page = useStorePage(project, search.tab, objectStorageTabs)
  const { tab, shown, gates, unreadable, deleter, setTab, setPod } = page
  const events = storeEvents(useProjectEvents(project), store)
  return (
    <StoreShell
      project={project}
      icon={HardDrivesIcon}
      name={store.name}
      state={<ObjectStorageState store={store} size="md" />}
      kind={`${t("s3.engine")}${store.version ? ` ${store.version}` : ""}`}
      created={store.created}
      updatedAt={updatedAt}
      failed={failed}
      tab={tab}
      tabs={shown.map((value) => ({ value, icon: tabIcons[value], label: t(tabLabel[value]) }))}
      tabsLabel={t("s3.tabs")}
      onTab={setTab}
      actions={
        <StoreMenu
          copy={`seaweeds/${store.name}`}
          deleteGate={gates("changeObjectStorage")}
          deleteLabel={t("s3.delete")}
          onDelete={() =>
            deleter.plan({
              title: t("title.delete", "Seaweed", store.name),
              changes: deleteObjectStorage(store),
            })
          }
        />
      }
    >
      <TabsContent value="overview">
        {tab === "overview" ? (
          <Overview
            project={project}
            store={store}
            events={events}
            onEvents={() => setTab("events")}
          />
        ) : null}
      </TabsContent>
      <TabsContent value="logs" className={tab === "logs" ? "flex min-h-0 flex-col" : undefined}>
        {tab === "logs" ? (
          <MemberLogs
            project={project}
            source={{ name: store.name, pods: store.pods, containers: [] }}
            pod={search.pod}
            onPodChange={setPod}
          />
        ) : null}
      </TabsContent>
      <TabsContent value="events">
        {tab === "events" ? (
          <Events events={events} pod={search.pod} unreadable={unreadable} onPod={setPod} />
        ) : null}
      </TabsContent>
      <TabsContent value="settings">
        {tab === "settings" ? (
          <ObjectStorageSettings
            project={project}
            store={store}
            mode={search.mode}
            onMode={page.setMode}
          />
        ) : null}
      </TabsContent>
      {deleter.dialog}
    </StoreShell>
  )
}

function Overview({
  project,
  store,
  events,
  onEvents,
}: {
  project: string
  store: ObjectStorage
  events: Event[]
  onEvents: () => void
}) {
  const t = useT()
  return (
    <div className="flex flex-col gap-7">
      <SummaryCard>
        <SummaryCardMain>
          <SummaryCardStatus tone={stateHealth[store.state]}>
            <SummaryCardWord>{t(`db.state.${store.state}`)}</SummaryCardWord>
          </SummaryCardStatus>
          <SummaryCardFacts>
            <SummaryCardFact label={t("s3.fact.parts")}>
              {t("db.membersReady", store.ready, store.members)}
            </SummaryCardFact>
            <SummaryCardFact label={t("db.fact.version")}>
              <span className="truncate" translate="no" title={store.image ?? undefined}>
                {store.version ?? "—"}
              </span>
            </SummaryCardFact>
            <SummaryCardFact label={t("db.fact.storage")}>
              <span translate="no">{store.storage ?? "—"}</span>
            </SummaryCardFact>
            <SummaryCardFact label={t("s3.fact.buckets")}>{store.buckets.length}</SummaryCardFact>
          </SummaryCardFacts>
        </SummaryCardMain>
        {store.message && store.state !== "ready" ? (
          <SummaryCardAside>
            <p className="type-meta text-muted-foreground">{t("db.fact.operator")}</p>
            <p className="text-sm wrap-anywhere">{store.message}</p>
          </SummaryCardAside>
        ) : null}
      </SummaryCard>
      <Alert variant="warn">
        <WarningIcon aria-hidden="true" />
        <AlertDescription>{t("s3.oneCopy")}</AlertDescription>
      </Alert>
      <KubeDetails
        facts={[
          [t("s3.fact.parts"), `Seaweed/${store.name} spec.master spec.volume spec.filer spec.s3`],
          [t("s3.fact.user"), store.iam.map((o) => `${o.kind}/${o.name}`).join(" ") || "—"],
        ]}
      />
      <Warnings events={events} onEvents={onEvents} />
      <Connection project={project} store={store} />
      <Buckets project={project} store={store} />
    </div>
  )
}

function Connection({ project, store }: { project: string; store: ObjectStorage }) {
  const t = useT()
  return (
    <Section id="s3-connect" title={t("db.connect")}>
      <ConnectServices project={project} database={connectable(store)} />
      {store.connection ? (
        <Credentials project={project} c={store.connection} accessKey={store.accessKey?.id} />
      ) : null}
    </Section>
  )
}

function Credentials({
  project,
  c,
  accessKey,
}: {
  project: string
  c: NonNullable<ObjectStorage["connection"]>
  accessKey?: string | null
}) {
  const t = useT()
  const revealed = useReveal(project, c.secret)
  const values = revealed.secret?.values
  const endpoint = `http://${c.host}:${c.port}`
  const id = accessKey ?? (c.usernameKey ? values?.[c.usernameKey]?.value : undefined)
  return (
    <>
      <p className="type-meta text-muted-foreground">{t("db.inCluster")}</p>
      <CardRows>
        <CardRow label={t("s3.endpoint")}>
          <Mono text={endpoint} />
          <CopyText value={() => endpoint} label={t("common.copy")} />
        </CardRow>
        <CardRow label={t("s3.region")}>
          <Mono text={REGION} />
        </CardRow>
        <CardRow label={t("s3.accessKey")}>
          {id ? (
            <>
              <Mono text={id} />
              <CopyText value={() => id} label={t("common.copy")} />
            </>
          ) : null}
        </CardRow>
        <RevealRow
          label={t("s3.secretKey")}
          value={values?.[c.passwordKey]?.value}
          reveal={revealed}
        />
      </CardRows>
      {revealed.secret?.problem ? (
        <Problem problem={revealed.secret.problem} what={c.secret} />
      ) : null}
    </>
  )
}

const phaseHealth = (phase?: string | null) =>
  phase === "Ready" ? "ok" : phase === "Failed" ? "fail" : phase ? "progress" : "idle"

function Buckets({ project, store }: { project: string; store: ObjectStorage }) {
  const t = useT()
  const gate = useGate(project, "changeObjectStorage")
  const planner = usePlanner(project)
  const { memory } = useQuantityFormat()
  const [adding, setAdding] = useState(false)
  const remove = (bucket: Bucket) =>
    planner.plan({
      title: t("s3.deleteBucket", bucket.s3Name),
      changes: deleteBucket(bucket),
      notes: [t(bucket.deletesContents ? "s3.deletesFiles" : "s3.keepsFiles", bucket.s3Name)],
    })
  return (
    <Section
      id="s3-buckets"
      title={t("s3.buckets")}
      action={
        <GateTip gate={gate}>
          <Button variant="outline" size="xs" onClick={() => setAdding(true)}>
            <PlusIcon aria-hidden="true" data-icon="inline-start" />
            {t("s3.newBucket")}
          </Button>
        </GateTip>
      }
    >
      {store.buckets.length === 0 ? (
        <p className="type-meta text-muted-foreground">{t("s3.noBuckets")}</p>
      ) : (
        <ul className="flex flex-col divide-y divide-main rounded-lg border border-main bg-card">
          {store.buckets.map((bucket) => (
            <li key={bucket.name} className="flex min-w-0 items-center gap-3 py-2 pr-2 pl-4">
              <div className="flex min-w-0 flex-1 flex-col">
                <span className="truncate type-mono" translate="no">
                  {bucket.s3Name}
                </span>
                <span className="truncate type-meta text-muted-foreground">
                  {bucket.usedBytes != null
                    ? t("s3.usage", memory(bucket.usedBytes), bucket.objects ?? 0)
                    : t("s3.noUsage")}
                </span>
                {bucket.message ? (
                  <span className="type-meta wrap-anywhere text-muted-foreground">
                    {bucket.message}
                  </span>
                ) : null}
              </div>
              {bucket.phase && bucket.phase !== "Ready" ? (
                <StatusPill health={phaseHealth(bucket.phase)} size="sm">
                  {bucket.phase}
                </StatusPill>
              ) : null}
              <GateTip gate={gate}>
                <Button
                  variant="ghost"
                  size="icon-xs"
                  aria-label={t("s3.deleteBucket", bucket.s3Name)}
                  onClick={() => remove(bucket)}
                >
                  <TrashIcon aria-hidden="true" />
                </Button>
              </GateTip>
            </li>
          ))}
        </ul>
      )}
      {adding ? (
        <NewBucketDrawer project={project} store={store} onClose={() => setAdding(false)} />
      ) : null}
      {planner.dialog}
    </Section>
  )
}

function NewBucketDrawer({
  project,
  store,
  onClose,
}: {
  project: string
  store: ObjectStorage
  onClose: () => void
}) {
  const t = useT()
  const [name, setName] = useState("")
  const [tried, setTried] = useState(false)
  const step = usePlanStep(project, onClose)
  const problem = bucketProblem(
    name,
    store.buckets.map((b) => b.s3Name),
  )
  const review = () => {
    setTried(true)
    if (problem) return
    step.plan({
      title: t("s3.planBucket", name, store.name),
      changes: createBucket(store.name, name),
    })
  }
  return (
    <PlanDrawer
      step={step}
      onClose={onClose}
      title={t("s3.newBucket")}
      footer={<ReviewFooter onCancel={onClose} onReview={review} />}
    >
      <ReviewForm onReview={review}>
        <NameField
          label={t("s3.field.bucket")}
          hint={t("s3.bucketRule")}
          error={tried && problem ? t(`s3.bucket.${problem}`) : undefined}
          value={name}
          onChange={setName}
        />
      </ReviewForm>
    </PlanDrawer>
  )
}
