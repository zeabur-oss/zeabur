// An apply refused because the project moved on after the review (a `409` before anything was
// written) plans the same change again and shows it; nothing more is written than was reviewed.
import type { Problem, Schemas } from "@/api/client"

/** The plan the person reviewed before the apply was refused. */
export type Reviewed = Pick<Schemas.PlanResponse, "objects">

export const replansOn = (problem: Problem): boolean =>
  problem.kind === "conflict" && !problem.managedBy && !problem.applied?.length

const named = (object: Schemas.PlannedObject) => `${object.target.kind} ${object.target.name}`

/** What the new plan writes that the reviewed one did not, and what it no longer writes. */
export function changedSince(
  reviewed: Reviewed,
  plan: Reviewed,
): { added: string[]; dropped: string[] } {
  const before = new Set(reviewed.objects.map(named))
  const after = new Set(plan.objects.map(named))
  return {
    added: [...after].filter((name) => !before.has(name)),
    dropped: [...before].filter((name) => !after.has(name)),
  }
}
