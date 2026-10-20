// What happened to a service, newest first. The API's timeline holds releases, writes by any
// tool, warnings and terminations; releases join it by revision for their pods and template.
// Without a timeline, releases and warning events stand in for it.
import type { Schemas } from "@/api/client"
import { isWarning, type Event } from "@/model/service"

type Release = Schemas["WorkloadRelease"]
export type TimelineEntry = Schemas["TimelineEntry"]

export type ChangeEntry =
  | { kind: "release"; at?: string; release: Release; entry?: TimelineEntry }
  | { kind: "warning"; at?: string; event: Event }
  | { kind: "timeline"; at: string; entry: TimelineEntry }

const time = (entry: ChangeEntry) => (entry.at ? Date.parse(entry.at) : 0)

export function changesOf(
  releases: Release[],
  events: Event[],
  timeline?: TimelineEntry[],
): ChangeEntry[] {
  const joined = new Set<number>()
  const fromTimeline = (timeline ?? []).map((entry): ChangeEntry => {
    const release = entry.release && releases.find((r) => r.revision === entry.release?.revision)
    if (!release) return { kind: "timeline", at: entry.at, entry }
    joined.add(release.revision)
    return { kind: "release", at: entry.at, release, entry }
  })
  return [
    ...fromTimeline,
    ...releases
      .filter((r) => !joined.has(r.revision))
      .map((release): ChangeEntry => ({
        kind: "release",
        at: release.created ?? undefined,
        release,
      })),
    ...(timeline
      ? []
      : events.filter(isWarning).map((event): ChangeEntry => ({
          kind: "warning",
          at: event.lastSeen ?? undefined,
          event,
        }))),
  ].toSorted(
    (a, b) =>
      time(b) - time(a) ||
      (a.kind === "release" && b.kind === "release" ? b.release.revision - a.release.revision : 0),
  )
}
