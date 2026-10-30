import { type UseMutationResult, useMutation, useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { useId, useState } from "react"
import { toast } from "sonner"
import ArrowSquareOutIcon from "~icons/ph/arrow-square-out"
import DesktopIcon from "~icons/ph/desktop"
import MoonIcon from "~icons/ph/moon"
import SunIcon from "~icons/ph/sun"

import { type Schemas } from "@/api/client"
import * as zeabur from "@/api/zeabur"
import { accountQuery } from "@/api/members"
import {
  capabilitiesQuery,
  clusterNameQuery,
  clusterQuery,
  metricsStoreQuery,
  queryClient,
  signInSettingQuery,
  whoamiQuery,
} from "@/api/queries"
import { Hint, Panel, Row, Rows, Segmented } from "@/components/bits"
import { Problem } from "@/components/problem"
import { RefusedKinds } from "@/components/refused"
import { StatusPill } from "@/components/status"
import { Button } from "@zeabur/design-system/components/button"
import { Input } from "@zeabur/design-system/components/input"
import { Label } from "@zeabur/design-system/components/label"
import { Select } from "@zeabur/design-system/components/select"
import { Switch } from "@zeabur/design-system/components/switch"
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

      <SignInPanel />

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
                <span className="font-mono text-xs" translate="no">
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

type NameSetting = Schemas.ClusterNameSetting

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

function NameForm({ setting, cluster }: { setting: NameSetting; cluster: Schemas.Cluster }) {
  const t = useT()
  const inputId = useId()
  const [draft, setDraft] = useState(cluster.name)
  const save = useMutation({
    mutationFn: async (name: string | null) => await zeabur.setClusterName({ name }),
    onSuccess: (next) => {
      queryClient.setQueryData(clusterNameQuery.queryKey, next)
      void queryClient.invalidateQueries({ queryKey: clusterQuery.queryKey, exact: true })
      toast.success(t("settings.storeSaved"))
    },
  })
  const keeps = Boolean(setting.storedIn)
  const editable = keeps && setting.canChange !== "denied"
  const changed = draft.trim() !== "" && draft.trim() !== cluster.name
  const source = <span className="text-xs text-muted-foreground">{nameSourceOf(cluster, t)}</span>
  const who = (
    <StoreWho
      id={`${inputId}-who`}
      storedIn={setting.storedIn}
      local={t("settings.clusterNameLocal")}
      denied={keeps && !editable}
    />
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
            {who}
          </div>
        )}
      </Row>
    </Rows>
  )
}

type StoreSetting = Schemas.MetricsStoreSetting

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
    mutationFn: async (service: string | null) => await zeabur.setMetricsStore({ service }),
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
  const changed = draft.trim() !== setting.service
  const isDefault = setting.service === setting.default
  const defaultMark = isDefault ? (
    <span className="text-xs text-muted-foreground">{t("settings.storeIsDefault")}</span>
  ) : null
  const who = (
    <StoreWho
      id={`${inputId}-who`}
      storedIn={setting.storedIn}
      local={t("settings.storeLocal")}
      denied={keeps && !editable}
    />
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

type SignInSetting = Schemas.SignInSetting

function SignInPanel() {
  const t = useT()
  const setting = useQuery(signInSettingQuery)
  return (
    <Panel id="settings-sign-in" title={t("settings.signIn")}>
      {setting.data ? (
        <SignInForm setting={setting.data} />
      ) : setting.isError ? (
        <Problem error={setting.error} />
      ) : (
        <p className="text-sm text-muted-foreground">…</p>
      )}
    </Panel>
  )
}

function SignInForm({ setting }: { setting: SignInSetting }) {
  const t = useT()
  const save = useMutation({
    mutationFn: async (body: Schemas.SignInChange) => await zeabur.setSignInSetting(body),
    onSuccess: (next) => {
      queryClient.setQueryData(signInSettingQuery.queryKey, next)
      toast.success(t("settings.storeSaved"))
    },
  })
  const editable = setting.canChange !== "denied" && setting.unreadable.length === 0
  return (
    <div className="flex flex-col gap-4">
      <p className="text-sm text-muted-foreground">{t("settings.signInLead")}</p>
      <RefusedKinds unreadable={setting.unreadable} line />
      <Rows>
        <TeamsRow setting={setting} editable={editable} save={save} />
        <AdminAal3Row setting={setting} editable={editable} save={save} />
      </Rows>
      <StoreWho storedIn={setting.storedIn} denied={!editable} />
      {save.isError ? <Problem error={save.error} what={t("settings.signIn")} /> : null}
    </div>
  )
}

type SignInSave = UseMutationResult<SignInSetting, Error, Schemas.SignInChange>
type SignInRowProps = { setting: SignInSetting; editable: boolean; save: SignInSave }

function TeamsRow({ setting, editable, save }: SignInRowProps) {
  const t = useT()
  return (
    <Row
      label={
        <>
          {t("settings.teams")}
          <Hint label={t("settings.teams")}>{t("settings.teamsHint")}</Hint>
        </>
      }
    >
      <div className="flex flex-col items-start gap-1.5">
        {editable ? (
          <Segmented
            label={t("settings.teams")}
            value={setting.teams}
            onChange={(teams) => {
              if (!save.isPending && teams !== setting.teams) save.mutate({ teams })
            }}
            options={(["single", "multiple"] as const).map((value) => ({
              value,
              label: t(`settings.teams.${value}`),
            }))}
          />
        ) : (
          <span>{t(`settings.teams.${setting.teams}`)}</span>
        )}
        <p className="text-xs text-muted-foreground">
          {setting.adminAal3 && setting.teams === "single"
            ? t("settings.teams.aal3")
            : t(`settings.teams.${setting.teams}Means`)}
        </p>
      </div>
    </Row>
  )
}

function AdminAal3Row({ setting, editable, save }: SignInRowProps) {
  const t = useT()
  const switchId = useId()
  // Turning it off never locks anyone out; turning it on needs a passkey of your own that counts.
  const mayTurnOn = setting.adminAal3 || setting.deviceBoundPasskey
  return (
    <Row label={t("settings.adminAccounts")}>
      <div className="flex flex-col gap-1.5">
        <div className="flex items-center gap-2">
          <Switch
            id={switchId}
            size="sm"
            checked={setting.adminAal3}
            disabled={!editable || !mayTurnOn || save.isPending}
            onCheckedChange={(adminAal3) => save.mutate({ adminAal3 })}
          />
          <Label htmlFor={switchId}>{t("settings.adminAal3")}</Label>
          <Hint label={t("settings.adminAal3Trust")}>{t("settings.adminAal3TrustHint")}</Hint>
        </div>
        <p className="text-xs text-muted-foreground">{t("settings.adminAal3Means")}</p>
        {editable && !mayTurnOn ? <AddDeviceBoundPasskey /> : null}
      </div>
    </Row>
  )
}

function StoreWho({
  id,
  storedIn,
  local,
  denied,
}: {
  id?: string
  storedIn?: { namespace?: string | null } | null
  local?: string
  denied: boolean
}) {
  const t = useT()
  return (
    <p id={id} className="text-xs text-muted-foreground">
      {storedIn ? t("settings.storeWho") : local}
      {denied ? ` ${t("settings.storeDenied")}` : ""}
      {storedIn ? (
        <Hint label={t("settings.storeWhereLabel")}>
          {t("settings.storeWhere", storedIn.namespace ?? "")}
        </Hint>
      ) : null}
    </p>
  )
}

function AddDeviceBoundPasskey() {
  const t = useT()
  const url = useQuery(accountQuery).data?.manageSignInUrl
  return (
    <p className="text-xs">
      {t("settings.adminAal3NeedsPasskey")}{" "}
      {url ? (
        <a
          href={url}
          target="_blank"
          rel="noreferrer"
          className="inline-flex items-center gap-1 text-foreground hover:underline"
        >
          {t("settings.adminAal3AddPasskey")}
          <ArrowSquareOutIcon aria-hidden="true" className="size-3" />
          <span className="sr-only">{t("common.newTab")}</span>
        </a>
      ) : (
        <Link to="/account" className="text-foreground hover:underline">
          {t("settings.adminAal3AddPasskey")}
        </Link>
      )}
    </p>
  )
}
