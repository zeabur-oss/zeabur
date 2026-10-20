// What happened to a service, newest first: its releases and the warnings around them. The API
// will add the plans applied through Zeabur and changes made elsewhere; they take the same shape.
import type { Schemas } from "@/api/client"
import { isWarning, type Event } from "@/model/service"

type Release = Schemas["WorkloadRelease"]

export type ChangeEntry =
  | { kind: "release"; at?: string; release: Release }
  | { kind: "warning"; at?: string; event: Event }
  | {
      kind: "plan" | "external"
      at: string
      /** Who made it: a person, or a tool such as Helm or Argo CD. */
      by?: string
      summary: string
    }

const time = (entry: ChangeEntry) => (entry.at ? Date.parse(entry.at) : 0)

export function changesOf(releases: Release[], events: Event[]): ChangeEntry[] {
  return [
    ...releases.map((release): ChangeEntry => ({
      kind: "release",
      at: release.created ?? undefined,
      release,
    })),
    ...events
      .filter(isWarning)
      .map((event): ChangeEntry => ({ kind: "warning", at: event.lastSeen ?? undefined, event })),
  ].toSorted(
    (a, b) =>
      time(b) - time(a) ||
      (a.kind === "release" && b.kind === "release" ? b.release.revision - a.release.revision : 0),
  )
}
