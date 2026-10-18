// Settings: preferences (language, theme), the metrics store usage history is read from, the
// signed-in person and About. Components are part of the cluster (/cluster/components).
import { useMutation, useQuery } from "@tanstack/react-query"
import { useId, useState } from "react"
import { toast } from "sonner"
import DesktopIcon from "~icons/ph/desktop"
import MoonIcon from "~icons/ph/moon"
import SunIcon from "~icons/ph/sun"

import { api, unwrap, type Schemas } from "@/api/client"
import {
  capabilitiesQuery,
  clusterQuery,
  metricsStoreQuery,
  queryClient,
  whoamiQuery,
} from "@/api/queries"
import { Hint, Panel, Segmented } from "@/components/bits"
import { Problem } from "@/components/problem"
import { permission, RefusedHint, useHintOffersElevation } from "@/components/refused"
import { StatusPill } from "@/components/status"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { localeNames, locales, useI18n, useT, type Locale } from "@/i18n"
import { setTheme, useTheme, type ThemeChoice } from "@/theme"

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
    <div className="mx-auto flex w-full max-w-4xl flex-col gap-6 px-4 py-6 md:px-8 md:py-8">
      <h1 className="text-2xl font-semibold tracking-tight">{t("settings.title")}</h1>

      <Panel title={t("settings.preferences")}>
        <div className="flex flex-col gap-5">
          <div className="grid grid-cols-1 gap-2 sm:grid-cols-settings sm:items-center">
            <label htmlFor={languageId} className="text-sm text-muted-foreground">
              {t("settings.language")}
            </label>
            <div className="flex items-center gap-1.5">
              <NativeSelect
                id={languageId}
                value={locale}
                onChange={(event) => setLocale(event.target.value as Locale)}
                className="w-64"
              >
                {locales.map((value) => (
                  <NativeSelectOption key={value} value={value} lang={value}>
                    {localeNames[value]}
                  </NativeSelectOption>
                ))}
              </NativeSelect>
              <Hint label={t("settings.language")}>{t("settings.languageHint")}</Hint>
            </div>
          </div>
          <div className="grid grid-cols-1 gap-2 sm:grid-cols-settings sm:items-center">
            <span className="text-sm text-muted-foreground">{t("settings.theme")}</span>
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
          </div>
        </div>
      </Panel>

      <MetricsStorePanel />

      <Panel title={t("settings.about")}>
        <dl className="grid grid-cols-1 gap-x-6 gap-y-3 text-sm sm:grid-cols-settings">
          <dt className="text-muted-foreground">{t("settings.user")}</dt>
          <dd translate="no">{whoami.data?.username ?? "—"}</dd>
          <dt className="text-muted-foreground">{t("settings.groups")}</dt>
          <dd className="flex flex-wrap gap-1.5">
            {whoami.data?.groups.map((group) => (
              <span key={group} className="rounded-md bg-muted px-1.5 py-0.5 font-mono text-xs">
                {group}
              </span>
            )) ?? "—"}
          </dd>
          {capabilities.data ? (
            <>
              <dt className="text-muted-foreground">{t("settings.kubernetesVersion")}</dt>
              <dd className="font-mono text-xs">{cluster.data?.kubernetesVersion ?? "—"}</dd>
              <dt className="text-muted-foreground">{t("settings.catalogueCommit")}</dt>
              <dd className="font-mono text-xs">
                {capabilities.data.catalogueCommit.slice(0, 12)}
              </dd>
            </>
          ) : null}
        </dl>
      </Panel>
    </div>
  )
}

type StoreSetting = Schemas["MetricsStoreSetting"]

const storeHealth = {
  available: "ok",
  "not-configured": "warn",
  unreadable: "idle",
  unavailable: "fail",
} as const satisfies Record<StoreSetting["state"], "ok" | "warn" | "idle" | "fail">

/** The metrics store: which Service history comes from, whether it answers, and, where engawa
 * keeps settings, a change of it written as the person (so who may is the cluster's decision). */
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
  return (
    <div className="flex flex-col gap-5">
      <div className="grid grid-cols-1 gap-2 sm:grid-cols-settings sm:items-start">
        <label
          htmlFor={inputId}
          className="flex items-center gap-1.5 pt-1.5 text-sm text-muted-foreground"
        >
          {t("settings.store")}
          <Hint label={t("settings.store")}>{t("settings.storeHint")}</Hint>
        </label>
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
              disabled={!editable || save.isPending}
              spellCheck={false}
              autoComplete="off"
              translate="no"
              className="min-w-64 flex-1"
              aria-describedby={`${inputId}-who`}
            />
            {editable ? (
              <>
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
              </>
            ) : null}
          </div>
          {keeps ? (
            <RefusedHint
              hint={setting.canChange}
              what={t("settings.storeChange")}
              missing={[permission("patch", "configmaps")]}
            />
          ) : null}
          <p id={`${inputId}-who`} className="text-xs text-muted-foreground">
            {setting.storedIn ? t("settings.storeWho") : t("settings.storeLocal")}
            {keeps && setting.canChange === "denied" && !elevation
              ? ` ${t("settings.storeDenied")}`
              : ""}
            {setting.storedIn ? (
              <Hint label={t("settings.storeWhereLabel")}>
                {t("settings.storeWhere", setting.storedIn.namespace ?? "")}
              </Hint>
            ) : null}
          </p>
          {save.isError ? <Problem error={save.error} what={t("settings.metricsStore")} /> : null}
        </form>
      </div>
      <dl className="grid grid-cols-1 gap-x-6 gap-y-3 text-sm sm:grid-cols-settings">
        <dt className="text-muted-foreground">{t("settings.storeState")}</dt>
        <dd className="flex flex-wrap items-center gap-2" data-store-state={setting.state}>
          <StatusPill health={storeHealth[setting.state]} size="sm">
            {t(`settings.storeState.${setting.state}`)}
          </StatusPill>
          {setting.source === "setting" ? (
            <span className="text-xs text-muted-foreground">{t("settings.storeChosen")}</span>
          ) : null}
          {setting.message ? (
            <Hint label={t("settings.storeState")}>
              <span className="font-mono" translate="no">
                {setting.message}
              </span>
            </Hint>
          ) : null}
        </dd>
        <dt className="text-muted-foreground">{t("settings.storeDefault")}</dt>
        <dd className="font-mono text-xs" translate="no">
          {setting.default}
        </dd>
      </dl>
    </div>
  )
}
