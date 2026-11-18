// Components an action needs and the cluster lacks, as `zeaburctl components export` names them.
import type { Schemas } from "@/api/client"

/** A component to install: its argument to `zeaburctl components export` and its name. */
export interface Needed {
  /** `capability/id`, or the capability alone when the catalogue has no component for it. */
  id: string
  name: string
}

/** `id` with the catalogue's name for it, else the last part of the id. A bare capability becomes
 * the component the catalogue offers for it: a recommended one, else the first. */
export function needed(id: string, capabilities: Schemas.Capability[] | undefined): Needed {
  const [capability = id, component] = id.split("/")
  const components = capabilities?.find((c) => c.name === capability)?.components ?? []
  const found = component
    ? components.find((c) => c.id === component)
    : (components.find((c) => Object.values(c.defaults).includes("recommended")) ?? components[0])
  return {
    id: found ? `${capability}/${found.id}` : id,
    name: found?.displayName ?? component ?? id,
  }
}
