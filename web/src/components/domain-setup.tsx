import CaretRightIcon from "~icons/ph/caret-right"

import type { Schemas } from "@/api/client"
import { CopyButton } from "@/components/bits"
import { StatusDot } from "@/components/status"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur/design-system/components/collapsible"
import { useI18n, useT } from "@/i18n"
import { formatDate } from "@/lib/format"
import { configuration, dnsSetup, recordsFor, wildcardFor, type DnsRecord } from "@/model/domains"

type DomainCheck = Schemas["DomainCheck"]

const configHealth = {
  valid: "ok",
  invalid: "fail",
  pending: "progress",
  elsewhere: "warn",
  unknown: "idle",
} as const

/** One line beside a domain: whether it is set up, and its certificate. */
export function DomainState({ check }: { check: DomainCheck }) {
  const t = useT()
  const { locale } = useI18n()
  const state = configuration(check)
  const certificate = check.certificate
  const cert =
    certificate?.state === "ready"
      ? certificate.notAfter
        ? t("domain.cert.until", formatDate(certificate.notAfter, locale))
        : t("domain.cert.ready")
      : undefined
  return (
    <span className="flex flex-wrap items-center gap-x-2 gap-y-0.5 type-meta text-secondary">
      <span className="flex items-center gap-1.5" role="status">
        <StatusDot health={configHealth[state]} />
        <span className={state === "invalid" ? "text-fail-strong" : undefined}>
          {t(`domain.${state}`)}
        </span>
      </span>
      {cert ? <span className="text-tertiary">{cert}</span> : null}
    </span>
  )
}

/** What to do about a domain that is not set up: the record to add, and the certificate's
 * trouble in cert-manager's words. Nothing when there is nothing to do. */
export function DomainSetup({
  host,
  check,
  suffix,
}: {
  host: string
  check: DomainCheck
  suffix?: string
}) {
  const t = useT()
  const dns = dnsSetup(host, check, suffix)
  const certificate = check.certificate
  const certTrouble =
    certificate && (certificate.state === "failed" || certificate.state === "issuing")
      ? certificate
      : undefined
  if (dns.as === "valid" && !certTrouble) return null
  const wildcard = wildcardFor(host, suffix)
  return (
    <div className="flex flex-col gap-3 text-sm">
      {dns.as === "valid" ? null : (
        <div className="flex flex-col gap-2">
          <p className="text-secondary">
            {dns.as === "missing"
              ? t("domain.dns.missing", host)
              : dns.as === "elsewhere"
                ? t("domain.dns.elsewhere", t("common.list", dns.resolved))
                : t("domain.dns.unknown")}
          </p>
          {dns.records.length === 0 ? (
            <p className="text-secondary">{t("domain.askAdmin")}</p>
          ) : (
            <>
              <p className="font-medium">
                {wildcard
                  ? t("domain.wildcardMissing", wildcard)
                  : dns.as === "missing"
                    ? t("domain.addRecord")
                    : t("domain.theRecord")}
              </p>
              <Records records={dns.records} />
            </>
          )}
        </div>
      )}
      {certTrouble ? (
        <div className="flex flex-col gap-1">
          <p className="flex items-center gap-1.5 font-medium">
            <StatusDot health={certTrouble.state === "failed" ? "fail" : "progress"} />
            {certTrouble.state === "failed" ? t("domain.cert.failed") : t("domain.cert.issuing")}
            {certTrouble.state === "failed" ? (
              <span className="font-normal text-secondary">{t("domain.cert.retries")}</span>
            ) : null}
          </p>
          {certTrouble.message ? (
            <Collapsible>
              <CollapsibleTrigger variant="disclosure">
                <CaretRightIcon aria-hidden="true" />
                {t("domain.cert.says")}
              </CollapsibleTrigger>
              <CollapsibleContent>
                <p className="pt-1 type-meta break-words text-secondary" lang="en" translate="no">
                  {certTrouble.message}
                </p>
              </CollapsibleContent>
            </Collapsible>
          ) : null}
        </div>
      ) : null}
    </div>
  )
}

/** Under the field that adds a domain: what its DNS will need. */
export function DnsHint({
  host,
  addresses,
  suffix,
}: {
  host: string
  addresses: string[]
  suffix?: string
}) {
  const t = useT()
  const wildcard = wildcardFor(host, suffix)
  if (wildcard) return <p className="type-meta text-secondary">{t("domain.wildcard", wildcard)}</p>
  const records = recordsFor(host, addresses)
  if (records.length === 0)
    return <p className="type-meta text-secondary">{t("domain.askAdmin")}</p>
  return (
    <div className="flex flex-col gap-2">
      <p className="type-meta text-secondary">{t("domain.afterAdding", host)}</p>
      <Records records={records} />
    </div>
  )
}

function Records({ records }: { records: DnsRecord[] }) {
  const t = useT()
  const cell = "flex min-w-0 items-center gap-1 font-mono text-xs"
  return (
    <div className="overflow-hidden rounded-md border border-v2-border bg-muted/30">
      <div className="grid grid-cols-record gap-x-3 border-b border-v2-border px-3 py-1.5 type-meta text-tertiary max-sm:hidden">
        <span>{t("domain.record.type")}</span>
        <span>{t("domain.record.name")}</span>
        <span>{t("domain.record.value")}</span>
      </div>
      <ul className="flex flex-col divide-y divide-v2-border">
        {records.map((record) => (
          <li
            key={`${record.type}/${record.value}`}
            className="grid grid-cols-record items-center gap-x-3 gap-y-1 px-3 py-1.5 max-sm:grid-cols-record-phone"
            translate="no"
          >
            <span className="font-mono text-xs font-medium">{record.type}</span>
            <span className={cell}>
              <span className="truncate" title={record.name}>
                {record.name}
              </span>
              <CopyButton value={record.name} />
            </span>
            <span className={`${cell} max-sm:col-start-2`}>
              <span className="truncate" title={record.value}>
                {record.value}
              </span>
              <CopyButton value={record.value} />
            </span>
          </li>
        ))}
      </ul>
    </div>
  )
}
