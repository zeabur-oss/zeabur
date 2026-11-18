// Components an action needs and the cluster lacks, as `zeaburctl components export` names them.
import type { Schemas } from "@/api/client"

/** A component to install: its argument to `zeaburctl components export` and its name. */
export interface Needed {
  /** The component's id, or the capability when the catalogue has no component for it. */
  id: string
  name: string
}

/** `id` with the catalogue's name for it. As in the catalogue, `id` names a component first, then a
 * capability, which becomes the component the catalogue offers for it: a recommended one, else
 * the first. */
export function needed(id: string, capabilities: Schemas.Capability[] | undefined): Needed {
  const all = capabilities?.flatMap((c) => c.components) ?? []
  const offered = capabilities?.find((c) => c.name === id)?.components ?? []
  const found =
    all.find((c) => c.id === id) ??
    offered.find((c) => Object.values(c.defaults).includes("recommended")) ??
    offered[0]
  return { id: found?.id ?? id, name: found?.displayName ?? id }
}
