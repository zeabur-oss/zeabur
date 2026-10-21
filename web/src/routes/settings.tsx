import { useMutation, useQuery } from "@tanstack/react-query"
import { useId, useState } from "react"
import { toast } from "sonner"
import DesktopIcon from "~icons/ph/desktop"
import MoonIcon from "~icons/ph/moon"
import SunIcon from "~icons/ph/sun"

import { api, unwrap, type Schemas } from "@/api/client"
import {
  capabilitiesQuery,
  clusterNameQuery,
  clusterQuery,
  metricsStoreQuery,
  queryClient,
  whoamiQuery,
} from "@/api/queries"
import { Hint, Panel, Row, Rows, Segmented } from "@/components/bits"
import { Problem } from "@/components/problem"
import { permission, RefusedHint, useHintOffersElevation } from "@/components/refused"
import { StatusPill } from "@/components/status"
import { Button } from "@zeabur/design-system/components/button"
import { Input } from "@zeabur/design-system/components/input"
import { Select } from "@zeabur/design-system/components/select"
import { localeNames, locales, useI18n, useT } from "@/i18n"
import { nameSourceOf } from "@/lib/cluster-name"
import { setTheme, useTheme, type ThemeChoice } from "@/theme"
import { PageHeader } from "@zeabur/design-system/components/page-header"
import { PageColumn } from "@zeabur/design-system/components/page-column"

export function SettingsPage() {
  const t = useT()
  const { locale, setLocale } = useI18n()
  const whoami = useQuery(whoamiQuery)
  const capabilities = useQuery(capabilitiesQuery)
  const cluster = useQuery(clusterQuery)
  const theme = useTheme()
  const languageId = useId()
  const themes: [ThemeChoice, typeof SunIcon][] = [
    ["system", DesktopIcon],
    ["light", SunIcon],
    ["dark", MoonIcon],
  ]
  return (
    <PageColumn width="content" gap="md" className="my-6 md:my-8">
      <PageHeader title={t("settings.title")} />

      <Panel title={t("settings.preferences")}>
        <Rows>
          <Row label={<label htmlFor={languageId}>{t("settings.language")}</label>}>
            <div className="flex items-center gap-1.5">
              <Select
                id={languageId}
                value={locale}
                onValueChange={setLocale}
                className="w-64"
                items={locales.map((value) => ({
                  value,
                  label: localeNames[value],
                  lang: value,
                }))}
              />
              <Hint label={t("settings.language")}>{t("settings.languageHint")}</Hint>
            </div>
          </Row>
          <Row label={t("settings.theme")}>
            <Segmented
              label={t("settings.theme")}
              value={theme}
              onChange={setTheme}
              options={themes.map(([value, ThemeIcon]) => ({
                value,
                label: (
                  <>
                    <ThemeIcon aria-hidden="true" />
                    {t(`theme.${value}`)}
                  </>
                ),
              }))}
            />
          </Row>
        </Rows>
      </Panel>

      <ClusterNamePanel />

      <MetricsStorePanel />

      <Panel title={t("settings.about")}>
        <Rows>
          <Row label={t("settings.user")}>
            <span translate="no">{whoami.data?.username ?? "—"}</span>
          </Row>
          <Row label={t("settings.groups")}>
            <span className="flex flex-wrap gap-1.5">
              {whoami.data?.groups.map((group) => (
                <span key={group} className="rounded-md bg-muted px-1.5 py-0.5 font-mono text-xs">
                  {group}
                </span>
              )) ?? "—"}
            </span>
          </Row>
          {capabilities.data ? (
            <>
              <Row label={t("settings.kubernetesVersion")}>
                <span className="font-mono text-xs">{cluster.data?.kubernetesVersion ?? "—"}</span>
              </Row>
              <Row label={t("settings.catalogueCommit")}>
                <span className="font-mono text-xs">
                  {capabilities.data.catalogueCommit.slice(0, 12)}
                </span>
              </Row>
            </>
          ) : null}
        </Rows>
      </Panel>
    </PageColumn>
  )
}

type NameSetting = Schemas["ClusterNameSetting"]

function ClusterNamePanel() {
  const t = useT()
  const setting = useQuery(clusterNameQuery)
  const cluster = useQuery(clusterQuery)
  return (
    <Panel id="settings-cluster-name" title={t("settings.clusterName")}>
      {setting.data && cluster.data ? (
        <NameForm
          key={`${setting.data.name ?? ""}/${cluster.data.name}`}
          setting={setting.data}
          cluster={cluster.data}
        />
      ) : setting.isError || cluster.isError ? (
        <Problem error={setting.error ?? cluster.error} />
      ) : (
        <p className="text-sm text-muted-foreground">…</p>
      )}
    </Panel>
  )
}

function NameForm({ setting, cluster }: { setting: NameSetting; cluster: Schemas["Cluster"] }) {
  const t = useT()
  const inputId = useId()
  const [draft, setDraft] = useState(cluster.name)
  const save = useMutation({
    mutationFn: async (name: string | null) =>
      unwrap(await api.PUT("/api/v1/settings/cluster-name", { body: { name } })),
    onSuccess: (next) => {
      queryClient.setQueryData(clusterNameQuery.queryKey, next)
      void queryClient.invalidateQueries({ queryKey: clusterQuery.queryKey, exact: true })
      toast.success(t("settings.storeSaved"))
    },
  })
  const keeps = Boolean(setting.storedIn)
  const editable = keeps && setting.canChange !== "denied"
  const elevation = useHintOffersElevation(setting.canChange)
  const changed = draft.trim() !== "" && draft.trim() !== cluster.name
  const source = <span className="text-xs text-muted-foreground">{nameSourceOf(cluster, t)}</span>
  const who =
    keeps && !editable && elevation ? null : (
      <p id={`${inputId}-who`} className="text-xs text-muted-foreground">
        {setting.storedIn ? t("settings.storeWho") : t("settings.clusterNameLocal")}
        {keeps && !editable ? ` ${t("settings.storeDenied")}` : ""}
        {setting.storedIn ? (
          <Hint label={t("settings.storeWhereLabel")}>
            {t("settings.storeWhere", setting.storedIn.namespace ?? "")}
          </Hint>
        ) : null}
      </p>
    )
  const label = (
    <>
      {editable ? (
        <label htmlFor={inputId}>{t("settings.clusterNameLabel")}</label>
      ) : (
        t("settings.clusterNameLabel")
      )}
      <Hint label={t("settings.clusterName")}>{t("settings.clusterNameHint")}</Hint>
    </>
  )
  return (
    <Rows>
      <Row label={label}>
        {editable ? (
          <form
            className="flex flex-col gap-2"
            onSubmit={(event) => {
              event.preventDefault()
              save.mutate(draft.trim())
            }}
          >
            <div className="flex flex-wrap items-center gap-2">
              <Input
                id={inputId}
                value={draft}
                onChange={(event) => setDraft(event.target.value)}
                disabled={save.isPending}
                autoComplete="off"
                maxLength={63}
                className="min-w-64 flex-1"
                aria-describedby={`${inputId}-who`}
                translate="no"
              />
              {source}
              <Button type="submit" size="sm" disabled={!changed || save.isPending}>
                {t("settings.storeSave")}
              </Button>
              {setting.name ? (
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  disabled={save.isPending}
                  onClick={() => save.mutate(null)}
                >
                  {t("settings.clusterNameReset")}
                </Button>
              ) : null}
            </div>
            {who}
            {save.isError ? <Problem error={save.error} what={t("settings.clusterName")} /> : null}
          </form>
        ) : (
          <div className="flex flex-col gap-2">
            <span className="flex flex-wrap items-baseline gap-x-2">
              <span className="break-all" translate="no">
                {cluster.name}
              </span>
              {source}
            </span>
            {keeps ? (
              <RefusedHint
                hint={setting.canChange}
                what={t("settings.clusterNameChange")}
                missing={[permission("patch", "configmaps")]}
              />
            ) : null}
            {who}
          </div>
        )}
      </Row>
    </Rows>
  )
}

type StoreSetting = Schemas["MetricsStoreSetting"]

const storeHealth = {
  available: "ok",
  "not-configured": "warn",
  unreadable: "idle",
  unavailable: "fail",
} as const satisfies Record<StoreSetting["state"], "ok" | "warn" | "idle" | "fail">

function MetricsStorePanel() {
  const t = useT()
  const store = useQuery(metricsStoreQuery)
  return (
    <Panel
      id="settings-metrics-store"
      title={t("settings.metricsStore")}
      action={<Hint label={t("settings.metricsStore")}>{t("settings.storeTenancy")}</Hint>}
    >
      {store.data ? (
        <StoreForm key={store.data.service} setting={store.data} />
      ) : store.isError ? (
        <Problem error={store.error} />
      ) : (
        <p className="text-sm text-muted-foreground">…</p>
      )}
    </Panel>
  )
}

function StoreForm({ setting }: { setting: StoreSetting }) {
  const t = useT()
  const inputId = useId()
  const [draft, setDraft] = useState(setting.service)
  const save = useMutation({
    mutationFn: async (service: string | null) =>
      unwrap(await api.PUT("/api/v1/settings/metrics-store", { body: { service } })),
    onSuccess: (next) => {
      queryClient.setQueryData(metricsStoreQuery.queryKey, next)
      // Every history was read from the store this replaces.
      void queryClient.invalidateQueries({
        predicate: (query) => query.queryKey[0] === "project" && query.queryKey[2] === "history",
      })
      toast.success(t("settings.storeSaved"))
    },
  })
  const keeps = Boolean(setting.storedIn)
  const editable = keeps && setting.canChange !== "denied"
  // The hint is checked with everyday rights; admin rights may allow the change.
  const elevation = useHintOffersElevation(setting.canChange)
  const changed = draft.trim() !== setting.service
  const isDefault = setting.service === setting.default
  const defaultMark = isDefault ? (
    <span className="text-xs text-muted-foreground">{t("settings.storeIsDefault")}</span>
  ) : null
  // A refusal that offers admin rights says who may change it; the line below would repeat it.
  const who =
    keeps && !editable && elevation ? null : (
      <p id={`${inputId}-who`} className="text-xs text-muted-foreground">
        {setting.storedIn ? t("settings.storeWho") : t("settings.storeLocal")}
        {keeps && !editable ? ` ${t("settings.storeDenied")}` : ""}
        {setting.storedIn ? (
          <Hint label={t("settings.storeWhereLabel")}>
            {t("settings.storeWhere", setting.storedIn.namespace ?? "")}
          </Hint>
        ) : null}
      </p>
    )
  return (
    <Rows>
      <Row
        label={
          <>
            {editable ? (
              <label htmlFor={inputId}>{t("settings.store")}</label>
            ) : (
              t("settings.store")
            )}
            <Hint label={t("settings.store")}>{t("settings.storeHint")}</Hint>
          </>
        }
      >
        {editable ? (
          <form
            className="flex flex-col gap-2"
            onSubmit={(event) => {
              event.preventDefault()
              save.mutate(draft.trim())
            }}
          >
            <div className="flex flex-wrap items-center gap-2">
              <Input
                id={inputId}
                value={draft}
                onChange={(event) => setDraft(event.target.value)}
                disabled={save.isPending}
                autoComplete="off"
                mono
                className="min-w-64 flex-1"
                aria-describedby={`${inputId}-who`}
              />
              {defaultMark}
              <Button type="submit" size="sm" disabled={!changed || save.isPending}>
                {t("settings.storeSave")}
              </Button>
              {setting.source === "setting" ? (
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  disabled={save.isPending}
                  onClick={() => save.mutate(null)}
                >
                  {t("settings.storeReset")}
                </Button>
              ) : null}
            </div>
            {who}
            {save.isError ? <Problem error={save.error} what={t("settings.metricsStore")} /> : null}
          </form>
        ) : (
          <div className="flex flex-col gap-2">
            <span className="flex flex-wrap items-baseline gap-x-2">
              <span className="type-mono break-all" translate="no">
                {setting.service}
              </span>
              {defaultMark}
            </span>
            {keeps ? (
              <RefusedHint
                hint={setting.canChange}
                what={t("settings.storeChange")}
                missing={[permission("patch", "configmaps")]}
              />
            ) : null}
            {who}
          </div>
        )}
      </Row>
      {setting.state === "unreadable" ? null : (
        <Row label={t("settings.storeState")}>
          <span className="flex flex-wrap items-center gap-2" data-store-state={setting.state}>
            <StatusPill health={storeHealth[setting.state]} size="sm">
              {t(`settings.storeState.${setting.state}`)}
            </StatusPill>
            {setting.source === "setting" ? (
              <span className="text-xs text-muted-foreground">{t("settings.storeChosen")}</span>
            ) : null}
            {setting.message ? (
              <span className="font-mono text-xs break-all text-muted-foreground" translate="no">
                {setting.message}
              </span>
            ) : null}
          </span>
        </Row>
      )}
      {isDefault ? null : (
        <Row label={t("settings.storeDefault")}>
          <span className="type-mono break-all" translate="no">
            {setting.default}
          </span>
        </Row>
      )}
    </Rows>
  )
}
