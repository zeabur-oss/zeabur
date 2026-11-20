// A service's security profile, Strict or Compatible: the Pod Security Standards level its pods
// meet (`restricted`, `baseline`). The server writes the pod settings of each.
import type { Schemas } from "@/api/client"

type Change = Schemas.Change
export type Level = Schemas.PodSecurityLevel
export type Profile = Exclude<Level, "privileged">

export const profiles: readonly Profile[] = ["restricted", "baseline"]

/** The profile for an image whose configuration sets `user`: Strict only for a numeric user other
 * than root, because the kubelet's `runAsNonRoot` check refuses a user name it cannot resolve. */
export function imageProfile(user: string | null | undefined): Profile {
  const uid = (user ?? "").split(":")[0]?.trim() ?? ""
  return /^\d+$/.test(uid) && Number(uid) !== 0 ? "restricted" : "baseline"
}

/** The change that makes an existing workload `profile`. */
export const profileChanges = (workload: Schemas.WorkloadRef, profile: Profile): Change[] => [
  { op: "securityProfile", workload, profile },
]
