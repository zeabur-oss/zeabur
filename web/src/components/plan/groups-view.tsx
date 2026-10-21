import { useState, type ReactNode } from "react"
import CaretRightIcon from "~icons/ph/caret-right"
import MagnifyingGlassIcon from "~icons/ph/magnifying-glass"

import {
  countRows,
  type EditGroup,
  type FieldRow,
  type OutcomeGroup,
  type RowKind,
} from "@/components/plan/groups"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur/design-system/components/collapsible"
import { TableContainer } from "@zeabur/design-system/components/table"
import { useT, type Translate } from "@/i18n"
import { cn } from "@/lib/utils"

/** From this many rows a filter box is offered. */
const FILTER_FROM = 12

/** The server's stand-in for a Secret's value (zeabur-graph's `HIDDEN`). */
const HIDDEN = "(hidden)"

interface Row {
  key: string
  cells: ReactNode[]
  text: string
}

interface Group {
  key: string
  object?: string
  title: string
  meta: string
  head: string[]
  rows: Row[]
  open: boolean
  /** Two-line on a phone: the first cell, then the rest with this between them. */
  joiner?: ReactNode
}

/** A plan's many changes in groups, each folding open into a bounded table. */
export function PlanGroups({
  edits,
  outcomes,
}: {
  edits?: readonly EditGroup[]
  outcomes?: readonly OutcomeGroup[]
}) {
  const t = useT()
  const groups = outcomes ? outcomeView(t, outcomes) : editView(t, edits ?? [])
  const total = groups.reduce((n, g) => n + g.rows.length, 0)
  const [query, setQuery] = useState("")
  const [open, setOpen] = useState(() => new Set(groups.filter((g) => g.open).map((g) => g.key)))
  const needle = query.trim().toLowerCase()
  const shown = groups
    .map((g) => (needle ? { ...g, rows: g.rows.filter((r) => r.text.includes(needle)) } : g))
    .filter((g) => g.rows.length > 0)
  let lastObject: string | undefined
  return (
    <div className="flex flex-col gap-3">
      {total >= FILTER_FROM ? (
        <label className="flex h-8 w-full items-center gap-2 rounded-lg border bg-background px-2.5 focus-within:border-ring sm:w-64">
          <MagnifyingGlassIcon className="size-4 text-muted-foreground" aria-hidden="true" />
          <input
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            aria-label={t("plan.filter")}
            className="w-full bg-transparent text-sm outline-none"
          />
        </label>
      ) : null}
      {shown.length === 0 ? (
        <p className="text-sm text-muted-foreground">{t("plan.noMatch")}</p>
      ) : null}
      {shown.map((group) => {
        const heading = group.object !== lastObject ? group.object : undefined
        lastObject = group.object
        return (
          <div key={group.key} className="flex flex-col gap-2">
            {heading ? (
              <h3 className="font-mono text-sm font-medium" translate="no">
                {heading}
              </h3>
            ) : null}
            <Collapsible
              open={needle ? true : open.has(group.key)}
              onOpenChange={(next) =>
                setOpen((previous) => {
                  const set = new Set(previous)
                  if (next) set.add(group.key)
                  else set.delete(group.key)
                  return set
                })
              }
            >
              <CollapsibleTrigger variant="disclosure">
                <CaretRightIcon aria-hidden="true" />
                <span className="font-medium text-main">{group.title}</span>
                <span className="ml-1 text-muted-foreground tabular-nums">{group.meta}</span>
              </CollapsibleTrigger>
              <CollapsibleContent>
                <div className="pt-2">
                  <RowsTable head={group.head} rows={group.rows} joiner={group.joiner} />
                </div>
              </CollapsibleContent>
            </Collapsible>
          </div>
        )
      })}
    </div>
  )
}

function RowsTable({ head, rows, joiner }: { head: string[]; rows: Row[]; joiner?: ReactNode }) {
  const columns = head.length === 3 ? "sm:grid-cols-3" : "sm:grid-cols-2"
  return (
    <TableContainer>
      <div role="table" className="max-h-80 overflow-y-auto text-sm">
        <div
          role="row"
          className={cn(
            "sticky top-0 z-1 hidden gap-3 border-b bg-card px-3 py-2 text-xs font-medium text-secondary sm:grid",
            columns,
          )}
        >
          {head.map((label) => (
            <span key={label} role="columnheader">
              {label}
            </span>
          ))}
        </div>
        <div role="rowgroup" className="divide-y">
          {rows.map((row) => {
            const [first, ...rest] = row.cells
            return (
              <div
                key={row.key}
                role="row"
                className={cn("grid min-w-0 gap-x-3 gap-y-0.5 px-3 py-2", columns)}
              >
                <span role="cell" className="flex min-w-0 items-baseline gap-1.5">
                  {first}
                </span>
                <div className="flex min-w-0 items-baseline gap-2 text-muted-foreground sm:contents sm:text-main">
                  {rest.map((cell, i) => (
                    <span key={i} className="contents">
                      {i > 0 && joiner ? (
                        <span aria-hidden="true" className="sm:hidden">
                          {joiner}
                        </span>
                      ) : null}
                      <span role="cell" className="min-w-0 truncate">
                        {cell}
                      </span>
                    </span>
                  ))}
                </div>
              </div>
            )
          })}
        </div>
      </div>
    </TableContainer>
  )
}

const sign: Record<RowKind, string> = { removed: "−", changed: "~", added: "+" }
const signTone: Record<RowKind, string> = {
  removed: "text-warn-strong",
  changed: "text-muted-foreground",
  added: "text-ok-strong",
}

function Value({ value }: { value?: string }) {
  const t = useT()
  if (value === undefined) return <span className="text-muted-foreground">—</span>
  if (value === HIDDEN)
    return <span className="text-muted-foreground italic">{t("plan.hidden")}</span>
  return (
    <span className="font-mono text-xs" translate="no" title={value}>
      {value === "" ? '""' : value}
    </span>
  )
}

function Field({ row }: { row: FieldRow }) {
  const t = useT()
  return (
    <>
      <span aria-hidden="true" className={cn("font-mono text-xs", signTone[row.kind])}>
        {sign[row.kind]}
      </span>
      <span className="sr-only">{t(`plan.${row.kind}`)}</span>
      <span className="min-w-0 truncate" title={[row.label, row.name].filter(Boolean).join(" ")}>
        {row.label ? <span className={row.name ? "mr-1.5" : undefined}>{row.label}</span> : null}
        {row.name ? (
          <span className="font-mono text-xs" translate="no">
            {row.name}
          </span>
        ) : null}
      </span>
    </>
  )
}

function editView(t: Translate, groups: readonly EditGroup[]): Group[] {
  const head = [t("plan.col.field"), t("plan.col.before"), t("plan.col.after")]
  return groups.map((group) => {
    const counts = countRows([group])
    const meta = (["removed", "changed", "added"] as const)
      .filter((kind) => counts[kind] > 0)
      .map((kind) => t(`plan.fact.${kind}`, counts[kind]))
      .join(t("common.sep"))
    const part = group.part === "other" ? t("plan.other") : t(`part.${group.part}`)
    return {
      key: group.key,
      object: group.object,
      title: group.container ? `${part} (${group.container})` : part,
      meta,
      head,
      open: counts.removed > 0 || groups.length <= 2,
      joiner: "→",
      rows: group.rows.map((row, i) => ({
        key: `${i}/${row.label ?? ""}/${row.name ?? ""}`,
        text: [row.label, row.name, row.before, row.after].join(" ").toLowerCase(),
        cells: [
          <Field key="field" row={row} />,
          <Value key="before" value={row.before} />,
          <Value key="after" value={row.after} />,
        ],
      })),
    }
  })
}

function outcomeView(t: Translate, groups: readonly OutcomeGroup[]): Group[] {
  return groups.map((group) => {
    const effects = group.rows.some((r) => r.effect)
    return {
      key: group.outcome,
      title: t(`plan.outcome.${group.outcome}.label`),
      meta: String(group.rows.length),
      head: [
        t("plan.col.service"),
        t("plan.col.replicas"),
        ...(effects ? [t("plan.col.effect")] : []),
      ],
      open: group.open,
      rows: group.rows.map((row) => ({
        key: `${row.workload.kind}/${row.workload.name}`,
        text: [row.workload.name, row.effect ?? ""].join(" ").toLowerCase(),
        cells: [
          <span key="name" className="truncate font-mono text-xs" translate="no">
            {row.workload.name}
          </span>,
          <span key="replicas" className="tabular-nums">
            {row.replicas}
          </span>,
          ...(effects ? [<span key="effect">{row.effect ?? ""}</span>] : []),
        ],
      })),
    }
  })
}
