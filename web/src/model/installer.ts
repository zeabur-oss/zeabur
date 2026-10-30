// Components an action needs and the cluster lacks, as zeabur-installer names them.
import type { Schemas } from "@/api/client"

/** A component to install: its entry in the installer's `enable` and its name. */
export interface Needed {
  /** `capability/id`, or a capability for the installer to fill. */
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
