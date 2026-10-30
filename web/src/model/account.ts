import type { Schemas } from "@/api/client"

export type ProjectAccess = Schemas.ProjectAccess
export type ProjectRole = Schemas.AccountRole

/** Roles a person can hold, highest first. */
export const roles = ["admin", "developer", "viewer"] as const satisfies readonly ProjectRole[]

/** From this many shown projects on, the list scrolls in its frame and has a filter. */
const TABLE_FROM = 11
/** Up to this many projects without a role are named; more are only counted. */
const NAMED_UP_TO = 7

export type AccessCounts = Record<(typeof roles)[number] | "none" | "unreadable", number>

export function accessCounts(projects: readonly ProjectAccess[]): AccessCounts {
  const counts: AccessCounts = { admin: 0, developer: 0, viewer: 0, none: 0, unreadable: 0 }
  for (const p of projects) counts[p.role ?? "unreadable"] += 1
  return counts
}

/** The rows of the list: projects whose access could not be read first, then by role and name.
 * Projects without a role are left out and said once, by `without`. */
export function accessRows(projects: readonly ProjectAccess[]): {
  rows: ProjectAccess[]
  without: { named: string[]; count: number }
} {
  const order: Record<ProjectRole, number> = { admin: 1, developer: 2, viewer: 3, none: 4 }
  const rank = (p: ProjectAccess) => (p.role ? order[p.role] : 0)
  const rows = projects
    .filter((p) => p.role !== "none")
    .toSorted((a, b) => rank(a) - rank(b) || a.name.localeCompare(b.name))
  const none = projects.filter((p) => p.role === "none").map(projectName)
  return {
    rows,
    without: { named: none.length <= NAMED_UP_TO ? none : [], count: none.length },
  }
}

export function accessLayout(count: number): "empty" | "list" | "table" {
  if (count === 0) return "empty"
  return count >= TABLE_FROM ? "table" : "list"
}

/** Projects whose name or shown name holds every word of `query`. */
export function filterAccess(projects: readonly ProjectAccess[], query: string): ProjectAccess[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean)
  if (words.length === 0) return [...projects]
  return projects.filter((p) => {
    const text = `${p.name} ${p.displayName ?? ""}`.toLowerCase()
    return words.every((word) => text.includes(word))
  })
}

export function projectName(project: Pick<ProjectAccess, "name" | "displayName">): string {
  return project.displayName?.trim() || project.name
}
