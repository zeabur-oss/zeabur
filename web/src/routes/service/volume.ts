// What the new-volume form sends: the claim name, access mode and StorageClass only when the
// person changed them, so the defaults (`<workload>-<volume>`, ReadWriteOnce, the cluster's
// class) stay in one place.
import type { Schemas } from "@/api/client"

/** The access modes a new claim may ask for, the default first. */
export const accessModes = [
  "ReadWriteOnce",
  "ReadWriteOncePod",
  "ReadWriteMany",
  "ReadOnlyMany",
] as const satisfies readonly Schemas["AccessMode"][]

/**
 * A DNS subdomain, the form of a claim's name:
 * https://kubernetes.io/docs/concepts/overview/working-with-objects/names/#dns-subdomain-names
 */
export const subdomain =
  /^(?=.{1,253}$)[a-z0-9]([-a-z0-9]*[a-z0-9])?(\.[a-z0-9]([-a-z0-9]*[a-z0-9])?)*$/

export interface VolumeValues {
  name: string
  mountPath: string
  size: string
  /** `null` while the person has not typed one: the field shows the default. */
  claimName: string | null
  accessMode: Schemas["AccessMode"]
  /** Absent for the cluster's default. */
  storageClass?: string
}

/** The claim name the server picks when none is given. */
export const defaultClaimName = (workload: string, volume: string): string =>
  volume.trim() === "" ? "" : `${workload}-${volume.trim()}`

export function volumeRequest(workload: string, values: VolumeValues): Schemas["VolumeRequest"] {
  const name = values.name.trim()
  const claimName = values.claimName?.trim()
  return {
    name: name || undefined,
    mountPath: values.mountPath.trim(),
    size: values.size.trim(),
    claimName:
      claimName === undefined || claimName === defaultClaimName(workload, name)
        ? undefined
        : claimName,
    accessMode: values.accessMode === "ReadWriteOnce" ? undefined : values.accessMode,
    ...(values.storageClass ? { storageClass: values.storageClass } : {}),
  }
}
