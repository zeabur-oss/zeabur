import { useEffect, useState } from "react"
import ArrowClockwiseIcon from "~icons/ph/arrow-clockwise"
import CheckCircleIcon from "~icons/ph/check-circle"
import PlusIcon from "~icons/ph/plus"
import WarningIcon from "~icons/ph/warning"
import XIcon from "~icons/ph/x"

import { api, type AccessMode } from "@/api"
import { Panel } from "@/components/bits"
import { useT } from "@/i18n"
import { Badge } from "@zeabur-oss/design-system/components/badge"
import { Button } from "@zeabur-oss/design-system/components/button"
import { Input } from "@zeabur-oss/design-system/components/input"

export const hostName =
  /^(?=.{1,253}$)([a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$/

/** The sslip.io domain that resolves to `address`, as the installer's plan makes it. */
export const sslipSuffix = (address: string) => `${address.replace(/[.:]/g, "-")}.sslip.io`

/** Same rule as the plan's `is_public`. */
export function isPublic(address: string): boolean {
  if (address.includes(":")) {
    const first = Number.parseInt(address.split(":")[0] || "0", 16)
    return (first & 0xe000) === 0x2000 && !address.toLowerCase().startsWith("2001:db8:")
  }
  const [a = 0, b = 0, c = 0] = address.split(".").map(Number)
  return !(
    a === 0 ||
    a === 10 ||
    a === 127 ||
    a >= 224 ||
    (a === 100 && (b & 0xc0) === 64) ||
    (a === 169 && b === 254) ||
    (a === 172 && (b & 0xf0) === 16) ||
    (a === 192 && b === 168) ||
    (a === 192 && b === 0 && c === 2) ||
    (a === 198 && b === 51 && c === 100) ||
    (a === 203 && b === 0 && c === 113)
  )
}

/** Whether the suffixes are all domain names: the step can go on. */
export const suffixesValid = (suffixes: string[] | undefined) =>
  (suffixes ?? []).every((s) => hostName.test(s))

export function SuffixesPanel({
  mode,
  suffixes,
  addresses,
  onChange,
}: {
  mode: AccessMode
  suffixes: string[] | undefined
  addresses: string[]
  onChange: (suffixes: string[] | undefined) => void
}) {
  const t = useT()
  const [round, setRound] = useState(0)
  const ipv4 = addresses.find((a) => !a.includes(":"))
  const nodeAddress = ipv4 ?? addresses[0]
  const publicAddress = nodeAddress && isPublic(nodeAddress) ? nodeAddress : undefined
  const list = suffixes ?? (publicAddress ? [sslipSuffix(publicAddress)] : [])
  const first = list.find((s) => hostName.test(s))
  const set = (index: number, value: string) =>
    onChange(list.map((s, i) => (i === index ? value.trim().toLowerCase() : s)))
  const privateSslip = nodeAddress && !publicAddress ? sslipSuffix(nodeAddress) : undefined
  return (
    <Panel
      title={t("access.suffixes")}
      action={
        list.length > 0 ? (
          <Button variant="outline" size="sm" onClick={() => setRound((n) => n + 1)}>
            <ArrowClockwiseIcon aria-hidden="true" data-icon="inline-start" />
            {t("access.recheck")}
          </Button>
        ) : null
      }
    >
      <div className="flex flex-col gap-4">
        <p className="max-w-prose text-sm text-secondary">
          {first ? t("access.suffixesLead", `web-shop.${first}`) : t("access.suffixesNone")}
        </p>
        {suffixes === undefined && privateSslip ? (
          <div className="flex max-w-prose flex-col items-start gap-2 rounded-lg border border-v2-border p-3 text-sm">
            {mode === "local" ? (
              <>
                <p className="text-secondary">{t("access.privateAsk", nodeAddress ?? "")}</p>
                <Button variant="outline" size="sm" onClick={() => onChange([privateSslip])}>
                  {t("access.privateUse", privateSslip)}
                </Button>
              </>
            ) : (
              <p className="text-secondary">{t("access.privateDomain", nodeAddress ?? "")}</p>
            )}
          </div>
        ) : null}
        {list.length > 0 ? (
          <ul className="flex flex-col gap-4">
            {list.map((suffix, index) => (
              <li key={index} className="flex flex-col gap-1.5">
                <div className="flex items-center gap-2">
                  <Input
                    value={suffix}
                    placeholder="apps.example.com"
                    mono
                    autoComplete="off"
                    spellCheck={false}
                    aria-label={t("access.suffix", index + 1)}
                    aria-invalid={suffix !== "" && !hostName.test(suffix)}
                    className="w-full min-w-0 max-w-md"
                    onChange={(event) => set(index, event.target.value)}
                  />
                  <Badge
                    variant="outline"
                    aria-hidden={index !== 0}
                    className={index === 0 ? "max-sm:hidden" : "invisible max-sm:hidden"}
                  >
                    {t("access.suffixDefault")}
                  </Badge>
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    aria-label={t("access.suffixRemove", suffix || String(index + 1))}
                    title={t("access.suffixRemove", suffix || String(index + 1))}
                    onClick={() => onChange(list.filter((_, i) => i !== index))}
                  >
                    <XIcon aria-hidden="true" />
                  </Button>
                </div>
                <SuffixCheck suffix={suffix} addresses={addresses} round={round} />
              </li>
            ))}
          </ul>
        ) : null}
        <div>
          <Button variant="outline" size="sm" onClick={() => onChange([...list, ""])}>
            <PlusIcon aria-hidden="true" data-icon="inline-start" />
            {t("access.suffixAdd")}
          </Button>
        </div>
      </div>
    </Panel>
  )
}

type Lookup = { addresses: string[] } | "checking"

function SuffixCheck({
  suffix,
  addresses,
  round,
}: {
  suffix: string
  addresses: string[]
  round: number
}) {
  const t = useT()
  const [lookup, setLookup] = useState<Lookup | null>(null)
  const valid = hostName.test(suffix)
  useEffect(() => {
    if (!valid) return
    let current = true
    setLookup("checking")
    // Only a wildcard record answers a name no one uses.
    const probe = `zeabur-check-${Math.random().toString(36).slice(2, 8)}.${suffix}`
    const timer = setTimeout(() => {
      api.resolve(probe).then(
        (found) => current && setLookup(found),
        () => current && setLookup(null),
      )
    }, 400)
    return () => {
      current = false
      clearTimeout(timer)
    }
  }, [suffix, valid, round])

  if (suffix !== "" && !valid) return <Line tone="bad">{t("access.invalid")}</Line>
  if (!valid || !lookup) return null
  if (lookup === "checking") return <Line>{t("access.checking")}</Line>
  const here = lookup.addresses.filter((a) => addresses.includes(a))
  if (here.length > 0) {
    return (
      <Line tone="good">
        {suffix.endsWith(".sslip.io") || suffix.endsWith(".nip.io")
          ? t("access.suffixSslip")
          : t("access.resolves", here.join(", "))}
      </Line>
    )
  }
  const wildcard = `*.${suffix}`
  const records = [
    addresses.find((a) => !a.includes(":")),
    addresses.find((a) => a.includes(":")),
  ].flatMap((a) => (a ? [{ type: a.includes(":") ? "AAAA" : "A", value: a }] : []))
  return (
    <div className="flex flex-col gap-1.5">
      <Line tone="bad">
        {lookup.addresses.length === 0
          ? t("access.suffixMissing", wildcard)
          : t("access.elsewhere", lookup.addresses.join(", "))}
      </Line>
      {records.length > 0 ? (
        <>
          <p className="text-xs text-secondary">{t("access.suffixRecord")}</p>
          <table className="w-full max-w-md text-left text-xs">
            <thead className="text-tertiary">
              <tr>
                <th className="py-1 pr-3 font-normal">{t("access.recordType")}</th>
                <th className="py-1 pr-3 font-normal">{t("access.recordName")}</th>
                <th className="py-1 font-normal">{t("access.recordValue")}</th>
              </tr>
            </thead>
            <tbody className="font-mono">
              {records.map((r) => (
                <tr key={r.type} className="border-t border-v2-separator">
                  <td className="py-1 pr-3">{r.type}</td>
                  <td className="py-1 pr-3 break-all select-all">{wildcard}</td>
                  <td className="py-1 break-all select-all">{r.value}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      ) : null}
    </div>
  )
}

function Line({ tone, children }: { tone?: "good" | "bad"; children: string }) {
  return (
    <span
      className={
        tone === "good"
          ? "flex items-start gap-1.5 text-xs text-ok-strong"
          : tone === "bad"
            ? "flex items-start gap-1.5 text-xs text-warn-strong"
            : "flex items-start gap-1.5 text-xs text-tertiary"
      }
    >
      {tone === "good" ? (
        <CheckCircleIcon className="mt-px size-3.5 shrink-0" aria-hidden="true" />
      ) : tone === "bad" ? (
        <WarningIcon className="mt-px size-3.5 shrink-0" aria-hidden="true" />
      ) : null}
      {children}
    </span>
  )
}
