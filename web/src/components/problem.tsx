import type { ReactNode } from "react"
import CaretRightIcon from "~icons/ph/caret-right"
import WarningCircleIcon from "~icons/ph/warning-circle"

import { ApiError, toProblem, type Problem as ProblemBody, type Schemas } from "@/api/client"
import { Ago } from "@/components/bits"
import { isRefusal, permission, Refused } from "@/components/refused"
import { Alert, AlertDescription, AlertTitle } from "@zeabur/design-system/components/alert"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur/design-system/components/collapsible"
import { useI18n, useT } from "@/i18n"
import { isKind, problemText } from "@/model/problem"

/** Shows an API problem (RFC 9457). The cluster's refusal is a normal state and is shown quietly
 * by `Refused`: `what` names what needs the rights, `retry` re-runs what refetching the page's
 * data does not. Any other problem is an alert in five layers (`Layers`): what happened, why,
 * what to do (with `action`, the page's way to do it), then the evidence and the raw answer,
 * folded. */
export function Problem({
  error,
  problem,
  retry,
  what,
  action,
}: {
  error?: unknown
  problem?: ProblemBody
  retry?: () => void
  what?: string
  action?: ReactNode
}) {
  const body = problem ?? toProblem(error)
  if (isRefusal(body)) return <Refused refusal={body} what={what} retry={retry} />
  const retryAfter = error instanceof ApiError ? error.retryAfter : undefined
  return <Failure problem={body} retryAfter={retryAfter} action={action} />
}

/** `retryAfter`: the wait a 429 or 503 asked for. A read's error is shown only once its retries
 * are spent, so the wait is said as the cluster's request, not as a promise to try again. */
function Failure({
  problem: body,
  retryAfter,
  action,
}: {
  problem: ProblemBody
  retryAfter?: number
  action?: ReactNode
}) {
  const t = useT()
  const { locale } = useI18n()
  const text = problemText(t, locale, body)
  const fix = !retryAfter ? text.fix : `${text.fix} ${t("problem.retryAfter", retryAfter)}`
  const status = body.kubernetes ?? undefined
  // Zeabur's sentence is often kube-apiserver's own message; it is said once.
  const folded = text.folded.filter((sentence) => sentence !== status?.message)
  const rows: RawRow[] = [
    ...(body.denied
      ? [
          [
            t("refused.missing"),
            permission(body.denied.verb, body.denied.resource, body.denied.group),
          ] as const,
        ]
      : []),
    ...(folded.length > 0 ? [[t("problem.detail"), folded.join(" "), "en"] as const] : []),
    ...(status
      ? [
          [t("problem.code"), String(status.code)] as const,
          [t("problem.reason"), status.reason] as const,
          [t("problem.message"), status.message] as const,
        ]
      : []),
  ]
  // The problem's type names a page that explains its rule (RFC 9457, section 3.1.1).
  const page = isKind(body.kind) && body.type.startsWith("/problems/") ? body.type : undefined
  return (
    <Alert variant="problem">
      <WarningCircleIcon aria-hidden="true" />
      <AlertTitle>{text.headline}</AlertTitle>
      <AlertDescription>
        <Layers
          why={text.why}
          whyLang={text.whyInEnglish ? "en" : undefined}
          quote={text.quote}
          fix={fix}
          action={action}
          evidence={body.evidence}
          fields={body.fields}
        >
          {rows.length > 0 || page ? (
            <>
              <Raw rows={rows} />
              {page ? (
                <a
                  href={page}
                  target="_blank"
                  rel="noopener"
                  className="w-fit text-xs text-foreground underline underline-offset-2"
                >
                  {t("problem.about")}
                  <span className="sr-only"> {t("common.newTab")}</span>
                </a>
              ) : null}
            </>
          ) : null}
        </Layers>
      </AlertDescription>
    </Alert>
  )
}

/**
 * Layers two to five of a problem or a diagnosis, under its headline: why (muted; a policy's own
 * words quoted beneath), what to do with the page's `action` beside it, then the evidence (each
 * value with where it was read, and the fields kube-apiserver named) and `children`, the raw
 * answer, each folded.
 */
export function Layers({
  why,
  whyLang,
  quote,
  fix,
  action,
  evidence,
  fields,
  lastSeen,
  children,
}: {
  why?: string
  whyLang?: string
  quote?: string
  fix: string
  action?: ReactNode
  evidence?: Schemas["Origin"][]
  fields?: Schemas["FieldCause"][]
  lastSeen?: string | null
  children?: ReactNode
}) {
  const t = useT()
  const proof = (evidence?.length ?? 0) + (fields?.length ?? 0) > 0
  return (
    <div className="flex flex-col gap-1.5">
      {why ? (
        <div className="text-muted-foreground" lang={whyLang}>
          {why}
        </div>
      ) : null}
      {quote ? (
        <blockquote className="border-l-2 pl-3 text-foreground" lang="en" translate="no">
          {quote}
        </blockquote>
      ) : null}
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5">
        <div className="min-w-0 flex-1 text-foreground">
          <span className="font-medium">{t("problem.whatToDo")}: </span>
          {fix}
        </div>
        {action}
      </div>
      {proof ? (
        <Fold
          label={t("diagnosis.evidence")}
          extra={lastSeen ? <Ago iso={lastSeen} className="ml-1.5" /> : null}
        >
          <Evidence evidence={evidence ?? []} fields={fields ?? []} />
        </Fold>
      ) : null}
      {children ? (
        <Fold label={t("problem.kubernetes")}>
          <div className="flex flex-col gap-2">{children}</div>
        </Fold>
      ) : null}
    </div>
  )
}

function Fold({
  label,
  extra,
  children,
}: {
  label: string
  extra?: ReactNode
  children: ReactNode
}) {
  return (
    <Collapsible>
      <CollapsibleTrigger variant="plain" className="group/trigger flex items-center">
        <CaretRightIcon
          aria-hidden="true"
          className="size-3 transition-transform group-data-[panel-open]/trigger:rotate-90 motion-reduce:transition-none"
        />
        {label}
        {extra}
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="pt-1.5 pb-0.5">{children}</div>
      </CollapsibleContent>
    </Collapsible>
  )
}

/** `Pod/web-1 status.containerStatuses[name=web].lastState`: where a value was read. */
function origin(o: Schemas["Origin"]): string {
  return [`${o.kind}/${o.name}`, o.fieldPath].filter(Boolean).join(" ")
}

/** Each value beside where it was read ("4  ResourceQuota/compute status.hard.requests.cpu"), and
 * each field kube-apiserver refused beside its message. */
function Evidence({
  evidence,
  fields,
}: {
  evidence: Schemas["Origin"][]
  fields: Schemas["FieldCause"][]
}) {
  return (
    <dl className="grid grid-cols-pairs gap-x-4 gap-y-1 font-mono text-xs">
      {evidence.map((e) => (
        <div key={origin(e)} className="contents">
          <dt className="text-foreground" translate="no">
            {e.value ?? "—"}
          </dt>
          <dd className="break-all text-muted-foreground" translate="no">
            {origin(e)}
          </dd>
        </div>
      ))}
      {fields.map((f) => (
        <div key={f.field} className="contents" data-field={f.field}>
          <dt className="text-foreground" translate="no">
            {f.field}
          </dt>
          <dd className="break-words text-muted-foreground" lang="en" translate="no">
            {f.message ?? f.reason}
          </dd>
        </div>
      ))}
    </dl>
  )
}

/** A label, a raw value, and the value's language when it is not the page's. */
export type RawRow = readonly [string, string] | readonly [string, string, string]

/** The raw layer: labelled values as Kubernetes gives them. */
export function Raw({ rows }: { rows: readonly RawRow[] }) {
  if (rows.length === 0) return null
  return (
    <dl className="grid grid-cols-pairs gap-x-4 gap-y-1 font-mono text-xs text-foreground">
      {rows.map(([label, value, lang]) => (
        <div key={label} className="contents">
          <dt className="text-muted-foreground">{label}</dt>
          <dd className="break-words" lang={lang} translate="no">
            {value}
          </dd>
        </div>
      ))}
    </dl>
  )
}
