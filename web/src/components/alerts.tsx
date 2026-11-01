// Falco's alerts as a list, newest first, repeats folded: the rule, where it fired and the command,
// with Falco's own sentence behind a disclosure.
import type { ReactNode } from "react"
import CaretRightIcon from "~icons/ph/caret-right"

import type { Schemas } from "@/api/client"
import { Ago } from "@/components/bits"
import { permission, Refused } from "@/components/refused"
import { StatusDot } from "@/components/status"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur-oss/design-system/components/collapsible"
import { useT } from "@/i18n"
import { folded, severityHealth, type Alert } from "@/model/alerts"

export function AlertList({
  alerts,
  limit,
  place,
}: {
  alerts: Alert[]
  limit?: number
  /** Where an alert fired, when the page knows more than the pod (its service). */
  place?: (alert: Alert) => ReactNode
}) {
  const t = useT()
  const rows = folded(alerts)
  const shown = limit ? rows.slice(0, limit) : rows
  return (
    <ol className="flex flex-col" aria-label={t("alerts.title")}>
      {shown.map(({ alert, count }) => (
        <li
          key={`${alert.origin.namespace ?? ""}/${alert.origin.name}/${alert.origin.fieldPath ?? ""}`}
          className="grid grid-cols-event gap-x-3 border-t border-rule py-2.5"
        >
          <StatusDot health={severityHealth[alert.severity]} className="mt-1.5" />
          <div className="flex min-w-0 flex-col gap-0.5">
            <div className="flex min-w-0 flex-wrap items-center gap-x-2 text-sm">
              <span className="font-semibold" lang="en" translate="no">
                {alert.rule}
              </span>
              <span className="type-meta text-muted-foreground">
                {t(`alerts.severity.${alert.severity}`)}
              </span>
              {count > 1 ? <span className="type-meta text-muted-foreground">×{count}</span> : null}
            </div>
            <div className="flex min-w-0 flex-wrap items-center gap-x-1.5 type-meta text-muted-foreground">
              {place?.(alert)}
              {alert.pod || alert.container ? (
                <span className="min-w-0 truncate font-mono" translate="no">
                  {[alert.pod, alert.container].filter(Boolean).join(t("common.sep"))}
                </span>
              ) : null}
            </div>
            {alert.command ? (
              <code className="truncate type-mono text-muted-foreground" translate="no">
                {alert.command}
              </code>
            ) : null}
            <Collapsible>
              <CollapsibleTrigger variant="disclosure">
                <CaretRightIcon aria-hidden="true" />
                {t("alerts.output")}
              </CollapsibleTrigger>
              <CollapsibleContent>
                <p className="pt-1 type-mono break-words" lang="en" translate="no">
                  {alert.output}
                </p>
              </CollapsibleContent>
            </Collapsible>
          </div>
          <Ago iso={alert.time} className="type-meta text-muted-foreground" />
        </li>
      ))}
    </ol>
  )
}

/** A report keeps only its newest alerts, so the list says so. */
export function RecentNote() {
  const t = useT()
  return <p className="type-meta text-muted-foreground">{t("alerts.recent")}</p>
}

export function AlertsRefused({ data }: { data: Schemas.RuntimeAlerts }) {
  const t = useT()
  if (data.state !== "refused") return null
  return (
    <Refused
      refusal="unreadable"
      what={t("alerts.seeing")}
      missing={data.unreadable.map((u) => permission("list", u.resource))}
      line
    />
  )
}
