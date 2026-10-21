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

/** Shown in the timeline before the rest folds into Older. */
export const RECENT = 7
/** A list this much longer than RECENT is shown whole: folding three rows saves nothing. */
const SLACK = 3
const WEEK = 7 * 24 * 3600e3

export const isTrouble = (entry: ChangeEntry) =>
  entry.kind === "warning" ||
  (entry.kind === "timeline" &&
    (entry.entry.kind === "warning" || entry.entry.kind === "termination"))

/** The newest changes and this week's warnings in the timeline, the rest under Older. */
export function foldChanges(
  entries: ChangeEntry[],
  now: number,
): { recent: ChangeEntry[]; older: ChangeEntry[] } {
  if (entries.length <= RECENT + SLACK) return { recent: entries, older: [] }
  const recent: ChangeEntry[] = []
  const older: ChangeEntry[] = []
  entries.forEach((entry, i) =>
    (i < RECENT || (isTrouble(entry) && time(entry) >= now - WEEK) ? recent : older).push(entry),
  )
  return { recent, older }
}

/** Releases and warnings in the last seven days. */
export function weekOf(
  entries: ChangeEntry[],
  now: number,
): { releases: number; warnings: number } {
  const week = entries.filter((e) => time(e) >= now - WEEK)
  return {
    releases: week.filter((e) => e.kind === "release").length,
    warnings: week.filter(isTrouble).length,
  }
}

/** What a row of Older says, for its filter. */
export function changeText(entry: ChangeEntry, sentence: (e: TimelineEntry) => string): string {
  switch (entry.kind) {
    case "release":
      return [
        `#${entry.release.revision}`,
        entry.release.changeCause ?? "",
        ...entry.release.images,
      ].join(" ")
    case "warning":
      return `${entry.event.reason} ${entry.event.note}`
    case "timeline":
      return sentence(entry.entry)
  }
}
