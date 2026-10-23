import { useState, type ReactElement, type ReactNode } from "react"
import CheckIcon from "~icons/ph/check"
import CopyIcon from "~icons/ph/copy"
import InfoIcon from "~icons/ph/info"

import { GateTip } from "@/components/gate"
import type { Gate } from "@/model/rights"
import { Button } from "@zeabur/design-system/components/button"
import { ToggleGroup, ToggleGroupItem } from "@zeabur/design-system/components/toggle-group"
import { Tooltip } from "@zeabur/design-system/components/tooltip"
import { useI18n, useT } from "@/i18n"
import { formatAgo, formatTime } from "@/lib/format"
import { cn } from "@/lib/utils"

export function Panel({
  title,
  action,
  children,
  className,
  bodyClassName,
  id,
}: {
  title: ReactNode
  action?: ReactNode
  children: ReactNode
  className?: string
  bodyClassName?: string
  id?: string
}) {
  return (
    <section
      aria-labelledby={id}
      className={cn(
        "flex min-w-0 flex-col overflow-hidden rounded-xl border border-v2-border bg-card",
        className,
      )}
    >
      <header className="flex min-h-12 flex-wrap items-center gap-2 border-b border-v2-border px-5 py-2">
        <h2 id={id} className="max-w-full shrink-0 truncate text-sm font-medium">
          {title}
        </h2>
        <div className="ml-auto flex flex-wrap items-center gap-1.5">{action}</div>
      </header>
      <div className={cn("flex-1 px-5 py-4", bodyClassName)}>{children}</div>
    </section>
  )
}

/** Labels and values in two columns from `sm`, every value at one left edge, in each `Row`. */
export function Rows({ children }: { children: ReactNode }) {
  return <dl className="grid gap-x-6 gap-y-4 text-sm sm:grid-cols-settings">{children}</dl>
}

/** A label and its value; the label lines up with the value's first line. */
export function Row({ label, children }: { label: ReactNode; children: ReactNode }) {
  return (
    <div className="grid gap-1.5 sm:col-span-2 sm:grid-cols-subgrid sm:items-baseline">
      <dt className="flex items-center gap-1.5 text-muted-foreground">{label}</dt>
      <dd className="min-w-0">{children}</dd>
    </div>
  )
}

export function Ago({ iso, className }: { iso?: string | null; className?: string }) {
  const { locale } = useI18n()
  if (!iso) return <span className={className}>—</span>
  return (
    <Tooltip content={formatTime(iso, locale)}>
      <time dateTime={iso} className={cn("whitespace-nowrap", className)}>
        {formatAgo(iso, locale)}
      </time>
    </Tooltip>
  )
}

export function CopyButton({ value, className }: { value: string; className?: string }) {
  const t = useT()
  const [copied, setCopied] = useState(false)
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(value)
      setCopied(true)
      window.setTimeout(() => setCopied(false), 1500)
    } catch {
      setCopied(false)
    }
  }
  const label = copied ? t("common.copied") : t("common.copy")
  return (
    <Button
      variant="muted"
      size="icon-xs"
      className={className}
      onClick={(event) => {
        event.stopPropagation()
        void copy()
      }}
      aria-label={label}
      title={label}
    >
      {copied ? <CheckIcon aria-hidden="true" /> : <CopyIcon aria-hidden="true" />}
    </Button>
  )
}

export function Hint({ children, label }: { children: ReactNode; label: string }) {
  return (
    <Tooltip content={children}>
      <button
        type="button"
        aria-label={label}
        className="inline-flex size-4 shrink-0 cursor-help items-center justify-center rounded-md align-glyph-middle text-muted-foreground outline-none hover:text-foreground"
      >
        <InfoIcon className="size-3.5" aria-hidden="true" />
      </button>
    </Tooltip>
  )
}

export interface Bucket {
  warning: number
  normal: number
}

export function Histogram({
  buckets,
  className,
  height = "h-12",
  labels,
}: {
  buckets: Bucket[]
  className?: string
  height?: string
  labels?: [string, string]
}) {
  // A few lines a bucket stay short bars, so a quiet, even stream does not fill the chart.
  const max = Math.max(4, ...buckets.map((b) => b.warning + b.normal))
  return (
    <div className={cn("flex flex-col gap-1", className)} aria-hidden="true">
      <div className={cn("flex items-end gap-0.75", height)}>
        {buckets.map((bucket, i) => {
          const total = bucket.warning + bucket.normal
          return (
            // oxlint-disable-next-line react/no-array-index-key -- buckets are positional
            <div key={i} className="flex h-full min-w-0 flex-1 flex-col justify-end">
              {total === 0 ? (
                <div className="h-px w-full bg-border" />
              ) : (
                <div
                  className="flex h-(--bar) w-full flex-col overflow-hidden rounded-xs"
                  style={{ "--bar": `${Math.max(8, (total / max) * 100)}%` }}
                >
                  <div className="grow-(--grow) bg-fail/85" style={{ "--grow": bucket.warning }} />
                  <div
                    className="grow-(--grow) bg-foreground/20"
                    style={{ "--grow": bucket.normal }}
                  />
                </div>
              )}
            </div>
          )
        })}
      </div>
      {labels ? (
        <div className="flex justify-between type-nano text-muted-foreground tabular-nums">
          <span>{labels[0]}</span>
          <span>{labels[1]}</span>
        </div>
      ) : null}
    </div>
  )
}

export function Segmented<T extends string>({
  value,
  options,
  onChange,
  label,
}: {
  value: T
  options: { value: T; label: ReactNode }[]
  onChange: (value: T) => void
  label: string
}) {
  return (
    <ToggleGroup
      aria-label={label}
      value={[value]}
      onValueChange={(next) => {
        // Pressing the chosen segment again would clear the group; a switch keeps its choice.
        const choice = options.find((option) => option.value === next[0])
        if (choice) onChange(choice.value)
      }}
      variant="segmented"
      spacing={0}
      className="shrink-0"
    >
      {options.map((option) => (
        <ToggleGroupItem key={option.value} value={option.value} size="sm" className="h-6.5">
          {option.label}
        </ToggleGroupItem>
      ))}
    </ToggleGroup>
  )
}

/** A control another tool owns, or the person's role cannot use, disabled, with why in a
 * tooltip. */
export function ManagedTip({
  tool,
  gate,
  children,
}: {
  tool?: string
  gate?: Gate
  children: ReactElement
}) {
  const t = useT()
  if (!tool) return gate ? <GateTip gate={gate}>{children}</GateTip> : children
  return (
    <Tooltip content={t("ops.managedEdit", tool)}>
      <span>{children}</span>
    </Tooltip>
  )
}
