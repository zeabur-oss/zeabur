import ArrowUpRightIcon from "~icons/ph/arrow-up-right"
import CaretRightIcon from "~icons/ph/caret-right"
import CheckCircleFillIcon from "~icons/ph/check-circle-fill"
import MinusCircleIcon from "~icons/ph/minus-circle"
import XCircleFillIcon from "~icons/ph/x-circle-fill"

import type { Schemas } from "@/api/client"
import { CopyButton } from "@/components/bits"
import { BrowserCheckLine } from "@/components/browser-check"
import { DomainSetup, DomainState } from "@/components/domain-setup"
import { StatusDot } from "@/components/status"
import { Badge } from "@zeabur-oss/design-system/components/badge"
import { Button } from "@zeabur-oss/design-system/components/button"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur-oss/design-system/components/collapsible"
import { useSentence, useT } from "@/i18n"
import { cn } from "@/lib/utils"
import { checkUrl } from "@/model/browser-check"
import { needsSetup } from "@/model/domains"
import { severity } from "@/model/severity"

type Route = Schemas.RouteTraceView
type HopStatus = Schemas.HopStatus

const statusIcon = {
  ok: CheckCircleFillIcon,
  failed: XCircleFillIcon,
  unknown: MinusCircleIcon,
} as const

const statusTone: Record<HopStatus, string> = {
  ok: "text-ok",
  failed: "text-fail",
  unknown: "text-tertiary",
}

const verdict = {
  ok: { health: "ok", key: "trace.reachable", tone: "text-secondary" },
  failed: { health: severity.unreachable, key: "trace.unreachable", tone: "text-fail-strong" },
  unknown: { health: "idle", key: "trace.unverified", tone: "text-secondary" },
} as const

/**
 * One address in a line with its verdict; where it stops, the hop in plain words and its fix.
 * The hops themselves wait behind a disclosure.
 */
export function RouteLine({
  route,
  waiting,
  fix,
  onFix,
}: {
  route: Route
  /** Said in place of the verdict when the address only waits on a cause shown elsewhere. */
  waiting?: string
  /** The diagnosis's fix for the hop that fails. */
  fix?: Schemas.Message
  /** Plans the change the API proposes for the hop that fails. */
  onFix?: () => void
}) {
  const t = useT()
  const write = useSentence()
  const stop = waiting ? undefined : route.hops.find((h) => h.status === "failed")
  const secure = route.hops.some((h) => h.step === "certificate" && h.status === "ok")
  const said = waiting
    ? { health: "idle" as const, text: waiting, tone: "text-secondary" }
    : { ...verdict[route.status], text: t(verdict[route.status].key) }
  const lead = conclusion(route, waiting)
  return (
    <div className="flex flex-col gap-2 px-4 py-3">
      <div className="flex min-w-0 items-center">
        {route.kind === "domain" ? (
          <a
            href={`${secure ? "https" : "http"}://${route.address}`}
            target="_blank"
            rel="noreferrer noopener"
            className="flex min-w-0 items-center gap-2 text-sm font-medium hover:underline"
            translate="no"
          >
            <span className="truncate">{route.address}</span>
            {secure ? (
              <Badge variant="ok" translate="no">
                HTTPS
              </Badge>
            ) : null}
            <ArrowUpRightIcon className="size-4 shrink-0 text-tertiary" aria-hidden="true" />
            <span className="sr-only"> {t("common.newTab")}</span>
          </a>
        ) : (
          <span className="flex min-w-0 items-center gap-1.5 text-sm">
            <span className="shrink-0 text-secondary">{t("status.private")}</span>
            <span className="truncate font-mono text-xs" translate="no" title={route.address}>
              {route.address}
            </span>
            <CopyButton value={route.address} />
          </span>
        )}
      </div>
      {lead.kind === "setup" ? (
        <div className="flex flex-col gap-2">
          <DomainState check={lead.check} size="text-sm" />
          <DomainSetup host={route.address} check={lead.check} />
        </div>
      ) : lead.kind === "browser" ? (
        <BrowserCheckLine address={route.address} tls={lead.tls} />
      ) : (
        <Verdict said={said} />
      )}
      {stop ? (
        <div className="flex flex-col gap-1 text-sm">
          <p>{write(stop.message)}</p>
          {fix ? (
            <p className="text-secondary">
              <span className="font-medium text-main">{t("trace.howToFix")}</span>
              {t("common.colon")}
              {write(fix)}
            </p>
          ) : null}
          {onFix ? (
            <Button variant="outline" size="sm" className="mt-1 self-start" onClick={onFix}>
              {t("trace.fix")}
            </Button>
          ) : null}
        </div>
      ) : null}
      <Collapsible>
        <CollapsibleTrigger variant="disclosure">
          <CaretRightIcon aria-hidden="true" />
          {t("trace.showChecks")}
        </CollapsibleTrigger>
        <CollapsibleContent>
          {lead.kind === "verdict" ? null : <Verdict said={said} className="pt-2" />}
          <Hops route={route} />
        </CollapsibleContent>
      </Collapsible>
    </div>
  )
}

type Lead =
  | { kind: "setup"; check: NonNullable<Route["domain"]> }
  | { kind: "browser"; tls: boolean }
  | { kind: "verdict" }

/** One conclusion: the record or certificate to fix first, else the browser's check where the
 * cluster could not tell, else the cluster's verdict. The rest waits in the checks. */
function conclusion(route: Route, waiting: string | undefined): Lead {
  if (waiting) return { kind: "verdict" }
  if (route.domain && needsSetup(route.domain)) return { kind: "setup", check: route.domain }
  const tls = route.hops.some((h) => h.step === "certificate")
  const unverified = route.kind === "domain" && route.status === "unknown"
  if (unverified && checkUrl(route.address, tls) !== undefined) return { kind: "browser", tls }
  return { kind: "verdict" }
}

function Verdict({
  said,
  className,
}: {
  said: { health: Parameters<typeof StatusDot>[0]["health"]; text: string; tone: string }
  className?: string
}) {
  return (
    <span className={cn("flex items-center gap-1.5 text-sm", said.tone, className)}>
      <StatusDot health={said.health} />
      {said.text}
    </span>
  )
}

/** Each hop from the address to a ready replica as Zeabur checked it. */
function Hops({ route }: { route: Route }) {
  const t = useT()
  const write = useSentence()
  return (
    <ol className="flex flex-col gap-2.5 pt-2 pl-1" aria-label={t("trace.title")}>
      {route.hops.map((hop, i) => {
        const Icon = statusIcon[hop.status]
        return (
          <li key={`${hop.step}-${i}`} className="flex gap-2">
            <Icon
              className={cn("mt-0.5 size-4 shrink-0", statusTone[hop.status])}
              aria-hidden="true"
            />
            <div className="flex min-w-0 flex-col gap-0.5 text-sm">
              <span className="flex flex-wrap items-baseline gap-x-2">
                <span className="font-medium">{t(`trace.step.${hop.step}`)}</span>
                <span
                  className={cn(
                    "type-meta",
                    hop.status === "failed" ? "text-fail-strong" : "text-tertiary",
                  )}
                >
                  {t(`trace.hop.${hop.status}`)}
                </span>
              </span>
              <span className="text-secondary">{write(hop.message)}</span>
              {hop.expected || hop.saw ? (
                <span className="flex flex-wrap gap-x-3 type-meta text-tertiary">
                  {hop.expected ? (
                    <span>
                      {t("trace.expected")}
                      {t("common.colon")}
                      <span className="font-mono" translate="no">
                        {hop.expected}
                      </span>
                    </span>
                  ) : null}
                  {hop.saw ? (
                    <span>
                      {t("trace.saw")}
                      {t("common.colon")}
                      <span className="font-mono" translate="no">
                        {hop.saw}
                      </span>
                    </span>
                  ) : null}
                </span>
              ) : null}
            </div>
          </li>
        )
      })}
    </ol>
  )
}
