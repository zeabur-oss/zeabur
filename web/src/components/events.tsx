// Kubernetes events as a list, newest first, repeats folded ("×14"). A long list (up to the 500 a
// page reads) scrolls in its own box and draws only the rows in view.
import { useVirtualizer } from "@tanstack/react-virtual"
import { useRef, type ReactNode } from "react"

import { Ago } from "@/components/bits"
import { StatusDot } from "@/components/status"
import { Term } from "@zeabur/design-system/components/term"
import { useT } from "@/i18n"
import { cn } from "@/lib/utils"
import { eventWord } from "@/model/overview"
import { isWarning, newestFirst, type Event } from "@/model/service"

export function EventList({
  events,
  limit,
  compact,
  plain,
  empty,
  objectLink,
}: {
  events: Event[]
  limit?: number
  /** Reason and object only, without the note. */
  compact?: boolean
  /** Plain words first for the reasons that have them, the reason beside them. */
  plain?: boolean
  empty?: string
  /** A link for the object an event is about (`pod/worker-…`), when the page knows where to go. */
  objectLink?: (event: Event, label: string) => ReactNode
}) {
  const t = useT()
  const sorted = events.toSorted(newestFirst)
  const shown = limit ? sorted.slice(0, limit) : sorted
  if (shown.length === 0)
    return <p className="py-3 text-sm text-muted-foreground">{empty ?? t("events.empty")}</p>
  const row = (event: Event, place?: Place) => {
    const warning = isWarning(event)
    const word = plain ? eventWord(event.reason) : undefined
    const object = event.regarding
      ? `${event.regarding.kind.toLowerCase()}/${event.regarding.name}`
      : undefined
    return (
      <li
        key={event.id}
        ref={place?.ref}
        data-index={place?.index}
        style={place ? { "--y": `${place.y}px` } : undefined}
        className={cn(
          "grid grid-cols-event gap-x-3 border-t border-rule",
          compact ? "py-2" : "py-3",
          place && "absolute inset-x-0 top-0 translate-y-(--y)",
        )}
      >
        <StatusDot health={warning ? "warn" : "idle"} className="mt-1.5" />
        <div className="flex min-w-0 flex-col gap-0.5">
          <div className="flex min-w-0 flex-wrap items-center gap-x-2 text-sm">
            <span className="font-semibold">
              <span className="sr-only">
                {warning ? t("events.warning") : t("events.normal")}:{" "}
              </span>
              {word ? <Term detail={event.reason}>{t(`event.word.${word}`)}</Term> : event.reason}
            </span>
            {event.count > 1 ? (
              <span className="type-meta text-muted-foreground">×{event.count}</span>
            ) : null}
            {object ? (
              <span className="min-w-0 truncate type-mono text-muted-foreground" translate="no">
                {objectLink ? objectLink(event, object) : object}
              </span>
            ) : null}
          </div>
          {compact ? null : (
            <p className="text-sm break-words text-muted-foreground">{event.note}</p>
          )}
        </div>
        <Ago iso={event.lastSeen} className="type-meta text-muted-foreground" />
      </li>
    )
  }

  if (shown.length > VIRTUAL_FROM)
    return <VirtualEvents events={shown} row={row} label={t("events.label")} />
  return (
    <ol className="flex flex-col" aria-label={t("events.label")}>
      {shown.map((event) => row(event))}
    </ol>
  )
}

/** Lists longer than this draw only the rows in view. */
const VIRTUAL_FROM = 100

/** Where a drawn row sits in a virtual list. */
interface Place {
  index: number
  y: number
  ref: (element: Element | null) => void
}

function VirtualEvents({
  events,
  row,
  label,
}: {
  events: Event[]
  row: (event: Event, place: Place) => ReactNode
  label: string
}) {
  const scroller = useRef<HTMLDivElement>(null)
  // oxlint-disable-next-line react/incompatible-library -- the virtualizer returns new functions each render
  const virtualizer = useVirtualizer({
    count: events.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => 72,
    overscan: 10,
  })
  return (
    <div ref={scroller} className="max-h-160 overflow-y-auto">
      <ol
        aria-label={label}
        className="relative w-full h-(--total)"
        style={{ "--total": `${virtualizer.getTotalSize()}px` }}
      >
        {virtualizer.getVirtualItems().map((item) => {
          const event = events[item.index]
          return event
            ? row(event, { index: item.index, y: item.start, ref: virtualizer.measureElement })
            : null
        })}
      </ol>
    </div>
  )
}
