// The name a person types to confirm a plan that deletes something.
import type { Schemas } from "@/api/client"

type Change = Schemas.Change

/** The name a person types to confirm a plan that deletes something: the project's when it
 * deletes `project`, else the first object it deletes; `undefined` when there is none. */
export function typedName(changes: readonly Change[], project?: string): string | undefined {
  for (const change of changes) {
    if (
      (change.op === "deleteProject" || change.op === "forceDeleteProject") &&
      project !== undefined
    )
      return project
    if (change.op === "delete") return change.name
  }
  return undefined
}

/** Whether a plan still waits for its name to be typed: it deletes something, and `typed` is
 * not that name (surrounding spaces aside; case counts, as Kubernetes names are lower case). */
export function awaitsName(changes: readonly Change[], typed: string, project?: string): boolean {
  const name = typedName(changes, project)
  return name !== undefined && typed.trim() !== name
}

const NAME_MARK = "\u0000"

/** The text before and after the name in the sentence `say` makes around one. */
export function aroundName(say: (name: string) => string): [string, string] {
  const [before = "", after = ""] = say(NAME_MARK).split(NAME_MARK)
  return [before, after]
}
