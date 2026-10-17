// One history chart, through shadcn/ui's Chart (Recharts): a line per replica (or the total),
// the reserved amount and the maximum as dashed lines, restarts as marks on the time axis, and a
// crosshair with every value at that time on hover or with the arrow keys (`accessibilityLayer`).
// Loaded on its own, after the page: Recharts is not part of the first load (scripts/ui-budget.sh).
//
// Colours are the theme's `--chart-1…5` (styles/shadcn/theme.css), named in the marks themselves.
// The ChartConfig carries labels only, so shadcn's ChartStyle writes no <style> element: it would
// need `dangerouslySetInnerHTML`, which the Trusted Types policy (`trusted-types 'none'`) refuses.
import { CartesianGrid, Line, LineChart, ReferenceDot, ReferenceLine, XAxis, YAxis } from "recharts"

import {
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
  type ChartConfig,
} from "@/components/ui/chart"
import { useI18n, useT } from "@/i18n"
import { colours, type Restart, type Series } from "@/model/usage"

/** A round number at or above `value` for the axis' top: a step of 1, 1.5, 2, 2.5, 3, 4, 5, 6 or 8 × 10ⁿ, in `unit`s (MiB for memory, so the top reads round). */
function niceCeil(value: number, unit: number): number {
  const units = value / unit
  if (units <= 0) return unit
  const power = 10 ** Math.floor(Math.log10(units))
  for (const step of [1, 1.5, 2, 2.5, 3, 4, 5, 6, 8])
    if (step * power >= units) return step * power * unit
  return 10 * power * unit
}

export interface Guide {
  kind: "reserved" | "max"
  value: number
}

/** Each series' key in the rows (a pod name may hold a `.`, which Recharts reads as a path). */
const key = (index: number) => `s${index}`

export default function HistoryChart({
  times,
  series,
  gone,
  guides,
  restarts,
  format,
  tick,
  unit,
  label,
  span,
}: {
  times: number[]
  series: Series[]
  /** Pods a rollout has replaced: drawn faded. */
  gone: Set<string>
  guides: Guide[]
  restarts: Restart[]
  format: (value: number) => string
  /** The axis' shorter numbers (the unit is in the chart's title). */
  tick: (value: number) => string
  /** What the axis' top is a round number of: 1 (a millicore) for CPU, 2²⁰ (a MiB) for memory. */
  unit: number
  label: string
  /** Seconds the chart covers, for the time labels. */
  span: number
}) {
  const t = useT()
  const { locale } = useI18n()
  const strokes = colours(series, gone)
  const rows = times.map((time, i) => {
    const row: Record<string, number | null> = { time }
    series.forEach((s, j) => {
      row[key(j)] = s.values[i] ?? null
    })
    return row
  })
  const config: ChartConfig = Object.fromEntries(
    series.map((s, j) => [key(j), { label: s.pod === "total" ? t("usage.total") : s.pod }]),
  )
  let top = 0
  for (const s of series) for (const v of s.values) if (v !== null && v > top) top = v
  for (const g of guides) if (g.value > top) top = g.value
  const axisTop = niceCeil(top * 1.05, unit)
  const axisTime = new Intl.DateTimeFormat(
    locale,
    span > 86_400
      ? { month: "numeric", day: "numeric" }
      : { hour: "2-digit", minute: "2-digit", hour12: false },
  )
  const fullTime = new Intl.DateTimeFormat(locale, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  })
  const guideText = (g: Guide) =>
    `${g.kind === "max" ? t("usage.maxLine") : t("usage.reservedLine")} ${format(g.value)}`
  return (
    <div className="flex flex-col gap-1">
      {/* The lines' values sit above the plot, not on it, where the series would cover them. */}
      {guides.length > 0 ? (
        <ul className="flex flex-wrap justify-end gap-x-3 text-xs text-muted-foreground">
          {guides.map((g) => (
            <li key={g.kind} className="flex items-center gap-1">
              <svg aria-hidden="true" width="14" height="2" className="overflow-visible">
                <line
                  x1="0"
                  x2="14"
                  y1="1"
                  y2="1"
                  stroke={g.kind === "max" ? "var(--fail)" : "var(--muted-foreground)"}
                  strokeDasharray={g.kind === "max" ? "6 4" : "2 4"}
                />
              </svg>
              <span className={g.kind === "max" ? "text-fail-strong" : undefined}>
                {guideText(g)}
              </span>
            </li>
          ))}
        </ul>
      ) : null}
      <ChartContainer config={config} className="aspect-auto h-44 w-full" aria-label={label}>
        <LineChart
          data={rows}
          accessibilityLayer
          margin={{ top: 12, right: 8, bottom: 0, left: 0 }}
        >
          <CartesianGrid vertical={false} />
          <XAxis
            dataKey="time"
            type="number"
            scale="time"
            domain={["dataMin", "dataMax"]}
            tickFormatter={(value: number) => axisTime.format(new Date(value * 1000))}
            tickLine={false}
            axisLine={false}
            minTickGap={32}
          />
          <YAxis
            domain={[0, axisTop]}
            ticks={[0, axisTop / 2, axisTop]}
            tickFormatter={(value: number) => tick(value)}
            tickLine={false}
            axisLine={false}
            width={64}
          />
          {guides.map((g) => (
            <ReferenceLine
              key={g.kind}
              y={g.value}
              stroke={g.kind === "max" ? "var(--fail)" : "var(--muted-foreground)"}
              strokeDasharray={g.kind === "max" ? "6 4" : "2 4"}
              ifOverflow="extendDomain"
            />
          ))}
          <ChartTooltip
            cursor
            content={
              <ChartTooltipContent
                indicator="line"
                labelFormatter={(_, payload) => {
                  const time = payload?.[0]?.payload?.time
                  return typeof time === "number" ? fullTime.format(new Date(time * 1000)) : ""
                }}
                formatter={(value, name) => (
                  <span className="flex w-full items-center justify-between gap-4">
                    <span className="font-mono text-muted-foreground" translate="no">
                      {config[String(name)]?.label}
                    </span>
                    <span className="font-mono tabular-nums">{format(Number(value))}</span>
                  </span>
                )}
              />
            }
          />
          {series.map((s, j) => (
            <Line
              key={s.pod}
              dataKey={key(j)}
              type="linear"
              stroke={s.pod === "total" ? "var(--progress)" : strokes[j]}
              strokeOpacity={gone.has(s.pod) ? 0.5 : 1}
              strokeWidth={s.pod === "total" ? 2 : 1.5}
              dot={false}
              connectNulls={false}
              isAnimationActive={false}
            />
          ))}
          {restarts.map((r) => (
            <ReferenceDot
              key={`${r.pod}@${r.at}`}
              x={r.at}
              y={0}
              r={4}
              fill={r.oomKilled ? "var(--fail)" : "var(--warn)"}
              stroke="var(--card)"
              strokeWidth={2}
              ifOverflow="visible"
            />
          ))}
        </LineChart>
      </ChartContainer>
    </div>
  )
}
