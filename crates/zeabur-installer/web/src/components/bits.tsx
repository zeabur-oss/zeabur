import { useLayoutEffect, useRef, useState, type ReactNode } from "react"
import CheckIcon from "~icons/ph/check"

import { Badge } from "@zeabur-oss/design-system/components/badge"
import { ToggleGroup, ToggleGroupItem } from "@zeabur-oss/design-system/components/toggle-group"
import { cn } from "@/lib/utils"

export type Health = "ok" | "progress" | "warn" | "fail" | "idle"

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

export function StatusPill({ health, children }: { health: Health; children: ReactNode }) {
  return (
    <Badge variant={health} size="lg">
      <StatusDot health={health} />
      {children}
    </Badge>
  )
}

export function Panel({
  title,
  action,
  children,
  flush,
  sticky,
}: {
  title: ReactNode
  action?: ReactNode
  children: ReactNode
  /** The body's rows reach the edges and divide themselves. */
  flush?: boolean
  /** The header stays in view while a long body scrolls past. */
  sticky?: boolean
}) {
  return (
    <section className="flex min-w-0 flex-col overflow-clip rounded-xl border border-v2-border bg-card">
      <header
        className={cn(
          "flex min-h-12 flex-wrap items-center gap-2 border-b border-v2-border px-5 py-2",
          sticky && "sticky top-0 z-10 bg-card",
        )}
      >
        <h2 className="max-w-full shrink-0 truncate text-sm font-medium">{title}</h2>
        <div className="ml-auto flex flex-wrap items-center gap-1.5">{action}</div>
      </header>
      <div className={flush ? "flex flex-col divide-y divide-v2-separator" : "px-5 py-4"}>
        {children}
      </div>
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
  const group = useRef<HTMLDivElement>(null)
  const [thumb, setThumb] = useState<{ x: number; width: number } | null>(null)
  // Segments are as wide as their labels, so the thumb follows the chosen one's measured box.
  useLayoutEffect(() => {
    const root = group.current
    if (!root) return
    const measure = () => {
      const item = root.querySelector<HTMLElement>("[aria-pressed='true']")
      setThumb(item ? { x: item.offsetLeft, width: item.offsetWidth } : null)
    }
    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(root)
    return () => observer.disconnect()
  }, [value, options.length])
  return (
    <ToggleGroup
      ref={group}
      aria-label={label}
      value={[value]}
      onValueChange={(next) => {
        // Pressing the chosen segment again would clear the group; a switch keeps its choice.
        const choice = options.find((option) => option.value === next[0])
        if (choice) onChange(choice.value)
      }}
      variant="segmented"
      spacing={0}
      className="relative flex w-fit shrink-0"
    >
      {thumb ? (
        <span
          aria-hidden="true"
          className="slide absolute inset-y-0.5 left-0 w-(--thumb) translate-x-(--slide-x) rounded-md bg-card shadow-xs ring ring-foreground/8 dark:bg-foreground/12"
          style={{ "--thumb": `${thumb.width}px`, "--slide-x": `${thumb.x}px` }}
        />
      ) : null}
      {options.map((option) => (
        <ToggleGroupItem
          key={option.value}
          value={option.value}
          size="sm"
          className="relative h-6.5"
        >
          {option.label}
        </ToggleGroupItem>
      ))}
    </ToggleGroup>
  )
}

/** The wizard's steps: finished ones ticked, the current one in violet. */
export function Steps({
  steps,
  current,
  label,
}: {
  steps: string[]
  current: number
  label: string
}) {
  return (
    <ol aria-label={label} className="flex min-w-0 items-center gap-2">
      {steps.map((step, i) => (
        <li
          key={step}
          aria-current={i === current ? "step" : undefined}
          className={cn(
            "flex min-w-0 items-center gap-2 text-sm",
            i === current ? "font-medium text-main" : "text-tertiary",
          )}
        >
          {i > 0 ? (
            <span aria-hidden="true" className="h-px w-3 shrink-0 bg-v2-border sm:w-6" />
          ) : null}
          <span
            aria-hidden="true"
            className={cn(
              "flex size-5 shrink-0 items-center justify-center rounded-full type-nano tabular-nums",
              i < current
                ? "bg-ok-soft text-ok-strong"
                : i === current
                  ? "bg-primary text-primary-foreground"
                  : "bg-surface-frame text-secondary",
            )}
          >
            {i < current ? <CheckIcon className="size-3" /> : i + 1}
          </span>
          <span className={i === current ? "truncate" : "hidden truncate md:inline"}>{step}</span>
        </li>
      ))}
    </ol>
  )
}

/** Machine text: a plan, a configuration file, a log. */
export function Code({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <pre
      className={cn(
        "overflow-x-auto rounded-lg bg-surface-frame px-4 py-3 type-mono text-xs leading-5 whitespace-pre text-main",
        className,
      )}
    >
      {children}
    </pre>
  )
}
