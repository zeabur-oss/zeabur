import type { ReactNode } from "react"

import { Badge } from "@zeabur/design-system/components/badge"
import { Tooltip } from "@zeabur/design-system/components/tooltip"
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

export function plainReason(t: Translate, reason: string | undefined, task: boolean) {
  const word = reasonWord(reason, task)
  return word ? t(`reason.${word}`) : undefined
}

export function useStatusWord() {
  const t = useT()
  return (status: Status) =>
    plainReason(t, status.reason, status.task ?? false) ??
    (status.unchecked ? t("status.readyUnchecked") : t(`status.${status.word}`))
}

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

export function StatusPill({
  health,
  size = "md",
  dot = true,
  className,
  children,
}: {
  health: Health
  size?: "sm" | "md"
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

function FactsPill({
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
