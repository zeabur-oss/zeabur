// A service's events: the list, repeats folded ("×14"). With a pod in the URL, only that pod's
// events, until the filter is cleared.
import XIcon from "~icons/ph/x"

import { EventList } from "@/components/events"
import { useT } from "@/i18n"
import type { Event } from "@/model/service"

export function EventsTab({
  events,
  pod,
  onClearPod,
}: {
  events: Event[]
  pod?: string
  onClearPod: () => void
}) {
  const t = useT()
  const shown = pod
    ? events.filter((e) => e.regarding?.kind === "Pod" && e.regarding.name === pod)
    : events
  return (
    <div className="flex flex-col gap-4">
      {pod ? (
        <div className="flex items-center gap-2 text-sm">
          <span className="text-muted-foreground">{t("events.forPod")}</span>
          <span
            className="inline-flex items-center gap-1 rounded-full border bg-muted/50 py-0.5 pr-1 pl-2.5 font-mono text-xs"
            translate="no"
          >
            {pod}
            <button
              type="button"
              onClick={onClearPod}
              aria-label={t("events.clearPod")}
              className="flex size-5 items-center justify-center rounded-full text-muted-foreground outline-none hover:bg-muted hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
            >
              <XIcon className="size-3" aria-hidden="true" />
            </button>
          </span>
        </div>
      ) : null}
      <div className="rounded-xl border bg-card px-4 py-2">
        <EventList events={shown} />
      </div>
    </div>
  )
}
