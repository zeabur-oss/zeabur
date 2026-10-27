// Components an action needs and the cluster lacks, as zeabur-installer names them.
import type { Schemas } from "@/api/client"

/** A component to install: its entry in the installer's `enable` and its name. */
export interface Needed {
  /** `capability/id`, or a capability for the installer to fill. */
  id: string
  name: string
}

/** Zeabur Builder, which brings zot. */
export const BUILDER = "builds/zeabur-builder"
export const KEEL = "image-updates/keel"
export const OBJECT_STORAGE = "object-storage/seaweedfs-operator"

/** `id` with the catalogue's name for it, else the last part of the id. */
export function needed(id: string, capabilities: Schemas["Capability"][] | undefined): Needed {
  const [capability = id, component] = id.split("/")
  const found = capabilities
    ?.find((c) => c.name === capability)
    ?.components.find((c) => c.id === component)
  return { id, name: found?.displayName ?? component ?? id }
}
