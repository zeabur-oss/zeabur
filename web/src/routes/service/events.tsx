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
            className="inline-flex items-center gap-1 rounded-md border bg-muted/50 py-0.5 pr-1 pl-2.5 font-mono text-xs"
            translate="no"
          >
            {pod}
            <button
              type="button"
              onClick={onClearPod}
              aria-label={t("events.clearPod")}
              className="flex size-5 items-center justify-center rounded-md text-muted-foreground outline-none hover:bg-muted hover:text-foreground"
            >
              <XIcon className="size-3" aria-hidden="true" />
            </button>
          </span>
        </div>
      ) : null}
      <div className="rounded-xl border border-v2-border bg-card px-4 py-2">
        <EventList events={shown} />
      </div>
    </div>
  )
}
