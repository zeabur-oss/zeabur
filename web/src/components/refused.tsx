import { useQuery } from "@tanstack/react-query"
import type { ReactNode } from "react"
import CaretRightIcon from "~icons/ph/caret-right"
import LockSimpleIcon from "~icons/ph/lock-simple"

import type { Problem, Schemas } from "@/api/client"
import { sessionQuery } from "@/api/queries"
import { retryAsAdmin } from "@/auth/elevation"
import { Button } from "@zeabur/design-system/components/button"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur/design-system/components/collapsible"
import { useT } from "@/i18n"
import { en } from "@/i18n/en"
import { cn } from "@/lib/utils"
import { isRefusal } from "@/model/problem"

/** `hint`: a permission hint said no before anything reached the cluster. */
export type Refusal = Problem | "unreadable" | "hint"

type Unreadable = Schemas["UnreadableView"]

export { isRefusal }

/** An answer read before the session's state was known is kube-apiserver's `forbidden` (it
 * carries `kubernetes`), not `elevation-required`. */
export function offersElevation(
  refusal: Refusal,
  elevation: Schemas["ElevationState"] | undefined,
): boolean {
  if (typeof refusal !== "string" && refusal.kind === "elevation-required")
    return elevation !== "active"
  if (elevation !== "available") return false
  return (
    typeof refusal === "string" || (refusal.kind === "forbidden" && Boolean(refusal.kubernetes))
  )
}

function useOffersElevation(refusal: Refusal): boolean {
  const session = useQuery(sessionQuery).data
  return offersElevation(refusal, session?.elevation)
}

/** The server checks hints with everyday rights, so `denied` is not final while admin rights are
 * offered. */
export function hintOffersElevation(
  hint: Schemas["Permission"] | undefined,
  elevation: Schemas["ElevationState"] | undefined,
): boolean {
  return hint === "denied" && offersElevation("hint", elevation)
}

export function useHintOffersElevation(hint: Schemas["Permission"] | undefined): boolean {
  const session = useQuery(sessionQuery).data
  return hintOffersElevation(hint, session?.elevation)
}

/** Granted rights re-read every query (auth/follow-elevation.ts); `retry` runs what that does not. */
export function RefusedHint({
  hint,
  what,
  missing,
  read,
  retry,
}: {
  hint: Schemas["Permission"] | undefined
  what: string
  missing?: string[]
  read?: boolean
  retry?: () => void
}) {
  const offered = useHintOffersElevation(hint)
  if (!offered) return null
  return (
    <Refused
      refusal={read ? "unreadable" : "hint"}
      what={what}
      missing={missing}
      retry={retry}
      line
    />
  )
}

export function permission(verb: string, resource: string, group = ""): string {
  return `${verb} ${group ? `${resource}.${group}` : resource}`
}

/** The catalogue's `fix` is what the line already says; only one written for the occasion is added. */
function serverSentences(problem: Problem): string[] {
  const own = problem.kind === "forbidden" || problem.kind === "elevation-required"
  const fix = own && problem.fix !== en[`problem.fix.${problem.kind}`] ? [problem.fix] : []
  return [problem.detail, ...fix].filter(Boolean)
}

/** `retry` re-runs what refetching the page does not (a POST, a reconnect). */
export function Refused({
  refusal,
  what,
  retry,
  missing,
  line,
  children,
}: {
  refusal: Refusal
  what?: string
  retry?: () => void
  missing?: string[]
  line?: boolean
  children?: ReactNode
}) {
  const t = useT()
  const offered = useOffersElevation(refusal)
  const problem = typeof refusal === "string" ? undefined : refusal
  const rules =
    missing ??
    (problem?.denied
      ? [permission(problem.denied.verb, problem.denied.resource, problem.denied.group)]
      : [])
  const sentences = problem ? serverSentences(problem) : []
  const status = problem?.kubernetes ?? undefined
  const details = rules.length > 0 || sentences.length > 0 || status || children
  const sentence = offered
    ? what
      ? t("refused.adminFor", what)
      : t("refused.admin")
    : what
      ? t("refused.rightsFor", what)
      : t("refused.rights")
  return (
    <div
      data-refused=""
      className={cn("text-sm", !line && "rounded-lg border border-dashed px-4 py-3")}
    >
      <Collapsible>
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5">
          <p className="flex min-w-0 items-start gap-2 text-muted-foreground">
            <LockSimpleIcon aria-hidden="true" className="mt-0.5 size-4 shrink-0" />
            <span>
              {sentence}
              {offered ? null : ` ${t("refused.ask")}`}
            </span>
          </p>
          {details ? (
            <CollapsibleTrigger variant="plain" className="group/trigger flex items-center">
              <CaretRightIcon
                aria-hidden="true"
                className="size-3 transition-transform group-data-[panel-open]/trigger:rotate-90 motion-reduce:transition-none"
              />
              {t("refused.details")}
            </CollapsibleTrigger>
          ) : null}
          {offered ? (
            <Button
              variant="outline"
              size="sm"
              className="ml-auto"
              onClick={() => void retryAsAdmin(retry)}
            >
              {refusal === "hint"
                ? t("elevate.use")
                : retry
                  ? t("elevate.retry")
                  : t("refused.view")}
            </Button>
          ) : null}
        </div>
        {details ? (
          <CollapsibleContent>
            <div className="flex flex-col gap-3 pt-3">
              <dl className="grid grid-cols-pairs gap-x-4 gap-y-1 font-mono text-xs">
                {rules.length > 0 ? (
                  <>
                    <dt className="text-muted-foreground">{t("refused.missing")}</dt>
                    <dd className="break-words" translate="no">
                      {rules.join(", ")}
                    </dd>
                  </>
                ) : null}
                {sentences.length > 0 ? (
                  <>
                    <dt className="text-muted-foreground">{t("problem.detail")}</dt>
                    <dd className="break-words" lang="en">
                      {sentences.join(" ")}
                    </dd>
                  </>
                ) : null}
                {status ? (
                  <>
                    <dt className="text-muted-foreground">{t("problem.code")}</dt>
                    <dd>{status.code}</dd>
                    <dt className="text-muted-foreground">{t("problem.reason")}</dt>
                    <dd>{status.reason}</dd>
                    <dt className="text-muted-foreground">{t("problem.message")}</dt>
                    <dd className="break-words">{status.message}</dd>
                  </>
                ) : null}
              </dl>
              {children}
            </div>
          </CollapsibleContent>
        ) : null}
      </Collapsible>
    </div>
  )
}

const resourceNames: Record<string, string> = {
  secrets: "Secrets",
  configmaps: "ConfigMaps",
  persistentvolumeclaims: "PersistentVolumeClaims",
  services: "Services",
  "ingresses.networking.k8s.io": "Ingresses",
  "httproutes.gateway.networking.k8s.io": "HTTPRoutes",
  "networkpolicies.networking.k8s.io": "NetworkPolicies",
  "deployments.apps": "Deployments",
  "statefulsets.apps": "StatefulSets",
  "daemonsets.apps": "DaemonSets",
  "jobs.batch": "Jobs",
  "cronjobs.batch": "CronJobs",
  pods: "Pods",
  "events.events.k8s.io": "Events",
}

export function refusedAmong(unreadable: Unreadable[], resources: readonly string[]): Unreadable[] {
  return unreadable.filter((u) => resources.includes(u.resource))
}

export function refusedElsewhere(
  unreadable: Unreadable[],
  placed: readonly string[],
): Unreadable[] {
  return unreadable.filter((u) => !placed.includes(u.resource))
}

export function RefusedKinds({ unreadable, line }: { unreadable: Unreadable[]; line?: boolean }) {
  const t = useT()
  if (unreadable.length === 0) return null
  const names = unreadable.map((u) => resourceNames[u.resource] ?? u.resource)
  return (
    <Refused
      refusal="unreadable"
      what={t("refused.see", t("common.list", names))}
      missing={unreadable.map((u) => permission("list", u.resource))}
      line={line}
    >
      <KubernetesAnswers unreadable={unreadable} />
    </Refused>
  )
}

export function RefusedSome({ unreadable }: { unreadable: Unreadable[] }) {
  const t = useT()
  if (unreadable.length === 0) return null
  return (
    <Refused
      refusal="unreadable"
      what={t("refused.some")}
      missing={unreadable.map((u) => permission("list", u.resource))}
      line
    >
      <KubernetesAnswers unreadable={unreadable} />
    </Refused>
  )
}

function KubernetesAnswers({ unreadable }: { unreadable: Unreadable[] }) {
  return (
    <ul className="flex flex-col gap-1 font-mono text-xs text-muted-foreground">
      {unreadable.map((u) => (
        <li key={u.resource} className="break-words">
          {u.kubernetes.code} {u.kubernetes.reason}: {u.kubernetes.message}
        </li>
      ))}
    </ul>
  )
}
