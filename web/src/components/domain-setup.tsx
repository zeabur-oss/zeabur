import ArrowClockwiseIcon from "~icons/ph/arrow-clockwise"
import CaretRightIcon from "~icons/ph/caret-right"

import type { Schemas } from "@/api/client"
import { CopyButton } from "@/components/bits"
import { StatusDot } from "@/components/status"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur-oss/design-system/components/collapsible"
import { useI18n, useT } from "@/i18n"
import { formatDate } from "@/lib/format"
import { cn } from "@/lib/utils"
import {
  configuration,
  dnsSetup,
  isSslip,
  rateLimited,
  recordsFor,
  wildcardFor,
  type DnsRecord,
} from "@/model/domains"
import { Button } from "@zeabur-oss/design-system/components/button"
import { severity } from "@/model/severity"

type DomainCheck = Schemas.DomainCheck

const configHealth = {
  valid: "ok",
  invalid: severity.unreachable,
  pending: severity.issuingCertificate,
  elsewhere: severity.dnsElsewhere,
  unknown: "idle",
} as const

/** One line beside a domain: whether it is set up, and its certificate. */
export function DomainState({
  check,
  size = "type-meta",
}: {
  check: DomainCheck
  size?: "type-meta" | "text-sm"
}) {
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
    <span className={cn("flex flex-wrap items-center gap-x-2 gap-y-0.5 text-secondary", size)}>
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
  suffixes,
  checking,
  onCheck,
}: {
  host: string
  check: DomainCheck
  suffixes?: readonly string[]
  checking?: boolean
  onCheck?: () => void
}) {
  const t = useT()
  const dns = dnsSetup(host, check, suffixes)
  const certificate = check.certificate
  const certTrouble =
    certificate && (certificate.state === "failed" || certificate.state === "issuing")
      ? certificate
      : undefined
  if (dns.as === "valid" && !certTrouble) return null
  const wildcard = wildcardFor(host, suffixes)
  const limited = certTrouble?.state === "failed" && rateLimited(certTrouble.message)
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
          {onCheck ? <CheckAgain checking={checking} onCheck={onCheck} /> : null}
        </div>
      )}
      {certTrouble ? (
        <div className="flex flex-col gap-1">
          <p className="flex items-center gap-1.5 font-medium">
            <StatusDot
              health={
                certTrouble.state === "failed"
                  ? severity.certificateFailed
                  : severity.issuingCertificate
              }
            />
            {certTrouble.state === "failed" ? t("domain.cert.failed") : t("domain.cert.issuing")}
            {certTrouble.state === "failed" ? (
              <span className="font-normal text-secondary">{t("domain.cert.retries")}</span>
            ) : null}
          </p>
          {limited ? (
            <p className="text-secondary">
              {isSslip(host) ? t("domain.cert.rateLimitedShared") : t("domain.cert.rateLimited")}
            </p>
          ) : null}
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

/** Looks the domain up again now instead of at the next poll. */
export function CheckAgain({ checking, onCheck }: { checking?: boolean; onCheck: () => void }) {
  const t = useT()
  return (
    <div>
      <Button size="xs" variant="outline" disabled={checking} onClick={onCheck}>
        <ArrowClockwiseIcon aria-hidden="true" data-icon="inline-start" />
        {checking ? t("domain.checking") : t("domain.checkAgain")}
      </Button>
    </div>
  )
}

/** Under the field that adds a domain: what its DNS will need. */
export function DnsHint({
  host,
  addresses,
  suffixes,
}: {
  host: string
  addresses: string[]
  suffixes?: readonly string[]
}) {
  const t = useT()
  if (isSslip(host)) return null
  const wildcard = wildcardFor(host, suffixes)
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

export function Records({ records }: { records: DnsRecord[] }) {
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
