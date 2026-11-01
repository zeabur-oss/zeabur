// Cluster → Overview → Domains: the domains new URLs are suggested under, whether a wildcard
// record sends each to the cluster, and the record to add when it does not.
import { useMutation, useQuery } from "@tanstack/react-query"
import { useState } from "react"
import { toast } from "sonner"
import PencilSimpleIcon from "~icons/ph/pencil-simple"
import PlusIcon from "~icons/ph/plus"
import XIcon from "~icons/ph/x"

import { type Schemas } from "@/api/client"
import * as zeabur from "@/api/zeabur"
import { clusterQuery, domainsQuery, queryClient } from "@/api/queries"
import { CheckAgain, Records } from "@/components/domain-setup"
import { GateTip, useHintGate } from "@/components/gate"
import { Problem } from "@/components/problem"
import { StatusPill } from "@/components/status"
import { useT } from "@/i18n"
import { recordsFor, sslipSuffix } from "@/model/domains"
import { Section } from "@/routes/cluster/section"
import { Badge } from "@zeabur-oss/design-system/components/badge"
import { Button } from "@zeabur-oss/design-system/components/button"
import { Input } from "@zeabur-oss/design-system/components/input"

type Setting = Schemas.DomainsSetting
type Suffix = Schemas.SuffixCheck

const health = {
  pointsHere: "ok",
  missing: "fail",
  elsewhere: "warn",
  unknown: "idle",
} as const satisfies Record<Suffix["dns"], "ok" | "fail" | "warn" | "idle">

/** The sslip.io suffix's address is no longer the cluster's: the suffix to move to. */
const movedTo = (suffix: Suffix, target: string[]) =>
  suffix.sslip && target.length > 0 && !target.includes(suffix.sslip)
    ? sslipSuffix(target[0] ?? "")
    : undefined

export function DomainsSection() {
  const t = useT()
  const query = useQuery(domainsQuery)
  const data = query.data
  const gate = useHintGate("changeDomainSuffixes", data?.canChange)
  const [editing, setEditing] = useState(false)
  const save = useMutation({
    mutationFn: async (suffixes: string[] | null) => await zeabur.setDomains({ suffixes }),
    onSuccess: (next) => {
      queryClient.setQueryData(domainsQuery.queryKey, next)
      void queryClient.invalidateQueries({ queryKey: clusterQuery.queryKey, exact: true })
      setEditing(false)
      toast.success(t("domains.saved"))
    },
  })
  if (!data) return null
  const editable = Boolean(data.storedIn)
  const first = data.suffixes[0]?.name
  return (
    <Section
      id="cluster-domains"
      title={t("domains.title")}
      count={data.suffixes.length || undefined}
      action={
        editing ? null : (
          <div className="flex items-center gap-2">
            {data.suffixes.length > 0 ? (
              <CheckAgain checking={query.isFetching} onCheck={() => void query.refetch()} />
            ) : null}
            {editable ? (
              <GateTip gate={gate}>
                <Button size="sm" variant="outline" onClick={() => setEditing(true)}>
                  <PencilSimpleIcon aria-hidden="true" data-icon="inline-start" />
                  {t("domains.edit")}
                </Button>
              </GateTip>
            ) : null}
          </div>
        )
      }
    >
      <p className="type-body text-secondary">
        {first ? (
          <>
            {t("domains.lead")}{" "}
            <span className="font-mono" translate="no">
              web-shop.{first}
            </span>
          </>
        ) : (
          t("domains.none")
        )}
      </p>
      {editing ? (
        <Editor
          setting={data}
          pending={save.isPending}
          onSave={(list) => save.mutate(list)}
          onReset={data.source === "setting" ? () => save.mutate(null) : undefined}
          onCancel={() => {
            save.reset()
            setEditing(false)
          }}
        />
      ) : data.suffixes.length > 0 ? (
        <ul className="flex flex-col divide-y divide-main rounded-lg border border-main bg-card">
          {data.suffixes.map((suffix, index) => (
            <Row
              key={suffix.name}
              suffix={suffix}
              setting={data}
              first={index === 0}
              onMove={
                editable && gate.open
                  ? (to) =>
                      save.mutate(data.suffixes.map((s) => (s.name === suffix.name ? to : s.name)))
                  : undefined
              }
            />
          ))}
        </ul>
      ) : null}
      {save.isError ? <Problem error={save.error} what={t("domains.title")} /> : null}
    </Section>
  )
}

function Row({
  suffix,
  setting,
  first,
  onMove,
}: {
  suffix: Suffix
  setting: Setting
  first: boolean
  onMove?: (to: string) => void
}) {
  const t = useT()
  const wildcard = `*.${suffix.name}`
  const records = recordsFor(wildcard, setting.target)
  const moved = movedTo(suffix, setting.target)
  const label =
    suffix.dns === "pointsHere" && suffix.sslip
      ? t("domains.state.sslip")
      : t(`domains.state.${suffix.dns}`)
  return (
    <li className="flex min-w-0 flex-col gap-2 px-4 py-2.5">
      <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
        <span className="min-w-0 truncate type-mono font-medium" translate="no">
          {suffix.name}
        </span>
        <StatusPill health={health[suffix.dns]} size="sm">
          {label}
        </StatusPill>
        {first ? <Badge variant="outline">{t("domains.default")}</Badge> : null}
      </div>
      {moved ? (
        <div className="flex flex-col items-start gap-2 text-sm">
          <p className="text-secondary">
            {t("domains.moved", suffix.sslip ?? "", setting.target[0] ?? "")}
          </p>
          {onMove ? (
            <Button size="xs" variant="outline" onClick={() => onMove(moved)}>
              {t("domains.useNew", moved)}
            </Button>
          ) : null}
        </div>
      ) : suffix.dns === "pointsHere" ? null : (
        <div className="flex flex-col gap-2 text-sm">
          <p className="text-secondary">
            {suffix.dns === "missing"
              ? t("domains.missing", wildcard)
              : suffix.dns === "elsewhere"
                ? t("domain.dns.elsewhere", t("common.list", suffix.resolved ?? []))
                : t("domain.dns.unknown")}
          </p>
          {records.length > 0 ? (
            <>
              <p className="font-medium">
                {suffix.dns === "missing" ? t("domain.addRecord") : t("domain.theRecord")}
              </p>
              <Records records={records} />
            </>
          ) : (
            <p className="text-secondary">{t("domain.askAdmin")}</p>
          )}
        </div>
      )}
    </li>
  )
}

const hostName =
  /^(\*\.)?([a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?\.?$/

function Editor({
  setting,
  pending,
  onSave,
  onReset,
  onCancel,
}: {
  setting: Setting
  pending: boolean
  onSave: (suffixes: string[]) => void
  onReset?: () => void
  onCancel: () => void
}) {
  const t = useT()
  const [rows, setRows] = useState(() => {
    const names = setting.suffixes.map((s) => s.name)
    return names.length > 0 ? names : [""]
  })
  const [tried, setTried] = useState(false)
  const cleaned = rows.map((r) => r.trim().toLowerCase()).filter(Boolean)
  const bad = cleaned.filter((r) => !hostName.test(r))
  const set = (index: number, value: string) =>
    setRows((list) => list.map((r, i) => (i === index ? value : r)))
  return (
    <form
      className="flex flex-col gap-4 rounded-xl border border-main bg-card p-5"
      onSubmit={(event) => {
        event.preventDefault()
        setTried(true)
        if (bad.length === 0) onSave(cleaned)
      }}
    >
      <p className="type-meta text-muted-foreground">{t("domains.editLead")}</p>
      <ul className="flex flex-col gap-2">
        {rows.map((row, index) => (
          <li key={index} className="flex items-center gap-2">
            <Input
              size="sm"
              mono
              value={row}
              placeholder="apps.example.com"
              autoComplete="off"
              spellCheck={false}
              aria-label={t("domains.suffix", index + 1)}
              aria-invalid={tried && row.trim() !== "" && !hostName.test(row.trim().toLowerCase())}
              className="min-w-0 flex-1"
              onChange={(event) => set(index, event.target.value)}
            />
            <Badge
              variant="outline"
              aria-hidden={index !== 0}
              className={index === 0 ? "max-sm:hidden" : "invisible max-sm:hidden"}
            >
              {t("domains.default")}
            </Badge>
            <Button
              type="button"
              size="icon-xs"
              variant="muted"
              aria-label={t("domains.remove", row || String(index + 1))}
              title={t("domains.remove", row || String(index + 1))}
              onClick={() => setRows((list) => list.filter((_, i) => i !== index))}
            >
              <XIcon aria-hidden="true" />
            </Button>
          </li>
        ))}
      </ul>
      {tried && bad.length > 0 ? (
        <p className="type-meta text-fail-strong">{t("domains.invalid", t("common.list", bad))}</p>
      ) : null}
      <div>
        <Button
          type="button"
          size="xs"
          variant="outline"
          onClick={() => setRows((list) => [...list, ""])}
        >
          <PlusIcon aria-hidden="true" data-icon="inline-start" />
          {t("domains.add")}
        </Button>
      </div>
      <div className="flex flex-wrap justify-end gap-2">
        {onReset ? (
          <Button
            size="sm"
            variant="ghost"
            type="button"
            disabled={pending}
            className="mr-auto"
            onClick={onReset}
          >
            {t("domains.reset")}
          </Button>
        ) : null}
        <Button size="sm" variant="ghost" type="button" onClick={onCancel}>
          {t("edit.cancel")}
        </Button>
        <Button size="sm" type="submit" disabled={pending}>
          {t("domains.save")}
        </Button>
      </div>
    </form>
  )
}
