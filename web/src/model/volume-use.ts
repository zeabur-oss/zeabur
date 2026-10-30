// How full a service's volumes are, from the kubelets' volume metrics in the metrics store. A
// StatefulSet's mount names a claim template (`data`); each replica has its own claim
// (`data-db-0`), and the fullest one counts.
import type { Schemas } from "@/api/client"
import { diskShare, fillHealth, VOLUME } from "@/model/node-health"
import { claimOf, type Health, type Service } from "@/model/service"

type VolumeUsage = Schemas.VolumeUsage
type Mount = Schemas.Mount

/** Each claim the service mounts once, with the container that mounts it. */
export function claimMounts(service: Service): { container: string; mount: Mount }[] {
  const seen = new Set<string>()
  return service.containers
    .filter((c) => !c.init)
    .flatMap((c) => c.mounts.map((mount) => ({ container: c.name, mount })))
    .filter(({ mount }) => {
      if (mount.kind !== "PersistentVolumeClaim" || seen.has(claimOf(mount))) return false
      seen.add(claimOf(mount))
      return true
    })
}

export interface VolumeFill {
  claim: string
  capacityBytes: number
  /** Taken: what is no longer free to write, so it reaches the capacity when writes fail. */
  takenBytes: number
  share: number
  health: Health
  /** Replicas whose claims were measured. */
  replicas: number
}

export type MissingReason = Exclude<Schemas.DiskState, "available">

/** Why some of `fills` are missing: the store's state, or that it holds nothing for them. */
export function missingReason(
  state: Schemas.DiskState,
  fills: readonly (VolumeFill | undefined)[],
): MissingReason | undefined {
  if (fills.every(Boolean)) return undefined
  return state === "available" ? "not-collected" : state
}

const belongs = (claim: string, template: string, workload: string) =>
  claim === template || claim.startsWith(`${template}-${workload}-`)

/** The fullest claim behind one mount, or nothing when none was measured. */
export function volumeFill(
  items: readonly VolumeUsage[],
  claim: string,
  workload: string,
): VolumeFill | undefined {
  const mine = items.filter((item) => belongs(item.claim, claim, workload))
  const fullest = mine
    .map((item) => ({ item, share: diskShare(item) ?? 0 }))
    .toSorted((a, b) => b.share - a.share)[0]
  if (!fullest) return undefined
  const { item, share } = fullest
  return {
    claim: item.claim,
    capacityBytes: item.capacityBytes,
    takenBytes: Math.max(0, item.capacityBytes - item.availableBytes),
    share,
    health: fillHealth(share, VOLUME),
    replicas: new Set(mine.map((m) => m.claim)).size,
  }
}
