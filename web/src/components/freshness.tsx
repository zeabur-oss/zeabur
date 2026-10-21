import { useQueryClient } from "@tanstack/react-query"
import { useEffect, useState } from "react"
import ArrowClockwiseIcon from "~icons/ph/arrow-clockwise"

import { StatusDot } from "@/components/status"
import { Button } from "@zeabur/design-system/components/button"
import { Tooltip } from "@zeabur/design-system/components/tooltip"
import { useI18n, useT } from "@/i18n"
import { formatTime, updatedWords } from "@/lib/format"
import { cn } from "@/lib/utils"

const spinAtLeast = 400

function useNow(every: number) {
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), every)
    return () => window.clearInterval(timer)
  }, [every])
  return now
}

/** "Updated 3m ago", kept current, with the exact time in its tooltip. */
export function UpdatedAt({ at }: { at: number }) {
  const t = useT()
  const { locale } = useI18n()
  const now = useNow(15_000)
  const iso = new Date(at).toISOString()
  return (
    <Tooltip content={formatTime(iso, locale)}>
      <time dateTime={iso} className="whitespace-nowrap">
        {updatedWords(t, at, Math.max(now, at))}
      </time>
    </Tooltip>
  )
}

/**
 * When the page last heard from the cluster, and a button that asks again. A failed refresh says
 * so instead of looking current, and the button stays to try again.
 */
export function Freshness({
  project,
  updatedAt,
  failed,
}: {
  project: string
  updatedAt: number
  failed: boolean
}) {
  const t = useT()
  const client = useQueryClient()
  const [refreshing, setRefreshing] = useState(false)
  if (!updatedAt) return null
  const refresh = async () => {
    setRefreshing(true)
    const started = Date.now()
    try {
      await client.invalidateQueries({
        queryKey: ["project", project],
        predicate: (query) => query.queryKey[2] !== "logs",
      })
    } finally {
      const left = spinAtLeast - (Date.now() - started)
      if (left > 0) await new Promise((resolve) => window.setTimeout(resolve, left))
      setRefreshing(false)
    }
  }
  return (
    <span role="status" className="flex items-center gap-1 type-meta text-muted-foreground">
      {failed ? (
        <span className="flex items-center gap-1.5 text-warn-strong">
          <StatusDot health="warn" />
          {t("freshness.failed")}
        </span>
      ) : null}
      <UpdatedAt at={updatedAt} />
      <Tooltip content={t("freshness.refresh")}>
        <Button
          variant="ghost"
          size="icon-xs"
          onClick={() => void refresh()}
          disabled={refreshing}
          aria-busy={refreshing}
          aria-label={t("freshness.refresh")}
        >
          <ArrowClockwiseIcon
            aria-hidden="true"
            className={cn(refreshing && "motion-safe:animate-spin motion-reduce:animate-pulse")}
          />
        </Button>
      </Tooltip>
    </span>
  )
}
