// Status as a pill (a dot and one plain word on a soft fill, "● Keeps crashing") where there is
// room, and a bare dot where there is not (the project column). Never colour alone: a dot always
// has its word next to it, in a tooltip or for screen readers. The kubelet's own word
// (`CrashLoopBackOff`) is in the pill's tooltip, with the restarts and the last failed exit.
import type { ReactNode } from "react"

import { Badge } from "@engawahq/design-system/components/badge"
import { Tooltip } from "@engawahq/design-system/components/tooltip"
import { useI18n, useT, type Translate } from "@/i18n"
import { formatAgo } from "@/lib/format"
import { cn } from "@/lib/utils"
import { reasonWord, type Health, type PodFacts, type Status } from "@/model/service"

const healthBg: Record<Health, string> = {
  ok: "bg-ok",
  progress: "bg-progress",
  warn: "bg-warn",
  fail: "bg-fail",
  idle: "bg-idle",
}

/** A 7px round dot; idle (scheduled, paused, completed) is a ring rather than a grey disc. */
export function StatusDot({ health, className }: { health: Health; className?: string }) {
  return (
    <span
      aria-hidden="true"
      className={cn(
        "inline-block size-1.75 shrink-0 rounded-full",
        health === "idle" ? "bg-transparent hairline-ring ring-idle ring-inset" : healthBg[health],
        className,
      )}
    />
  )
}

/** A raw reason in plain words ("Keeps crashing"), or undefined when it has none. */
export function plainReason(t: Translate, reason: string | undefined, task: boolean) {
  const word = reasonWord(reason, task)
  return word ? t(`reason.${word}`) : undefined
}

/** The status in one plain word: what the failing pod's reason means, else the status word. */
export function useStatusWord() {
  const t = useT()
  return (status: Status) =>
    plainReason(t, status.reason, status.task ?? false) ?? t(`status.${status.word}`)
}

/** What a status rests on, for its tooltip: "CrashLoopBackOff, 14 restarts", or, for a pod that
 * runs again after a recent crash, "Running, 5 restarts, last exit 1, 4 minutes ago". */
export function useStatusFacts() {
  const t = useT()
  const { locale } = useI18n()
  return (reason: string | undefined, pod: PodFacts | undefined): string | undefined => {
    const parts = [
      pod?.running ? t("phase.Running") : reason,
      pod && pod.restarts > 0 ? t("pod.restarts", pod.restarts) : undefined,
      pod?.running && pod.lastExit
        ? t(
            "status.lastExit",
            pod.lastExit.code,
            pod.lastExit.finished ? formatAgo(pod.lastExit.finished, locale) : "",
          )
        : undefined,
    ].filter((part): part is string => !!part)
    return parts.length > 0 ? parts.join(t("common.sep")) : undefined
  }
}

/** shadcn/ui's Badge in a status tone (its variant per health: the soft fill behind the strong
 * text), with its dot. `sm` for table rows. */
export function StatusPill({
  health,
  size = "md",
  dot = true,
  className,
  children,
}: {
  health: Health
  size?: "sm" | "md"
  /** Leave the dot out where the pill is a count, not a state ("1 unused"). */
  dot?: boolean
  className?: string
  children: ReactNode
}) {
  return (
    <Badge variant={health} size={size === "md" ? "lg" : "sm"} className={className}>
      {dot ? <StatusDot health={health} /> : null}
      {children}
    </Badge>
  )
}

/** A service's status as a pill; `counts` adds "3/3" after the word. */
export function ServiceStatus({
  status,
  size = "md",
  counts = false,
  className,
}: {
  status: Status
  size?: "sm" | "md"
  counts?: boolean
  className?: string
}) {
  const word = useStatusWord()
  const facts = useStatusFacts()(status.reason, status.pod)
  return (
    <FactsPill health={status.health} size={size} facts={facts} className={className}>
      {word(status)}
      {counts && status.desired !== undefined && status.ready !== undefined ? (
        <span className="opacity-70">
          {status.ready}/{status.desired}
        </span>
      ) : null}
    </FactsPill>
  )
}

/** A status pill whose plain word rests on `facts` (the raw reason, the restarts): shown in a
 * tooltip on hover, and read with the word by a screen reader. */
export function FactsPill({
  health,
  size,
  dot,
  facts,
  className,
  children,
}: {
  health: Health
  size?: "sm" | "md"
  dot?: boolean
  facts?: string
  className?: string
  children: ReactNode
}) {
  const t = useT()
  const pill = (
    <StatusPill health={health} size={size} dot={dot} className={className}>
      {children}
      {facts ? <span className="sr-only">{`${t("common.sep")}${facts}`}</span> : null}
    </StatusPill>
  )
  if (!facts) return pill
  return (
    <Tooltip content={facts}>
      <span className="inline-flex">{pill}</span>
    </Tooltip>
  )
}
