// A service's security profile, Strict or Compatible: the Pod Security Standards level its pods
// meet (`restricted`, `baseline`). Mirrors crates/zeabur-plan/src/pod_security.rs.
import type { Schemas } from "@/api/client"

type Change = Schemas["Change"]
type Manifest = Record<string, unknown>
export type Level = Schemas["PodSecurityLevel"]
export type Profile = Exclude<Level, "privileged">

export const profiles: readonly Profile[] = ["restricted", "baseline"]

/** What Strict writes: the restricted standard's settings. */
export const RESTRICTED_FIELDS =
  "runAsNonRoot: true, seccompProfile: RuntimeDefault, allowPrivilegeEscalation: false, capabilities.drop: [ALL]"

/** The pod template fields a new service gets. Neither profile mounts the service account's
 * token: an ordinary service never calls the Kubernetes API. Compatible still keeps the
 * container from gaining privileges and keeps the runtime's seccomp profile; only root is let
 * back in. */
export function templateSecurity(profile: Profile): { pod: Manifest; container: Manifest } {
  const seccompProfile = { type: "RuntimeDefault" }
  return profile === "restricted"
    ? {
        pod: {
          automountServiceAccountToken: false,
          securityContext: { runAsNonRoot: true, seccompProfile },
        },
        container: {
          securityContext: { allowPrivilegeEscalation: false, capabilities: { drop: ["ALL"] } },
        },
      }
    : {
        pod: { automountServiceAccountToken: false, securityContext: { seccompProfile } },
        container: { securityContext: { allowPrivilegeEscalation: false } },
      }
}

/** The profile for an image whose configuration sets `user`: Strict only for a numeric user other
 * than root, because the kubelet's `runAsNonRoot` check refuses a user name it cannot resolve. */
export function imageProfile(user: string | null | undefined): Profile {
  const uid = (user ?? "").split(":")[0]?.trim() ?? ""
  return /^\d+$/.test(uid) && Number(uid) !== 0 ? "restricted" : "baseline"
}

/** The change that makes an existing workload `profile`. */
export const profileChanges = (workload: Schemas["WorkloadRef"], profile: Profile): Change[] => [
  { op: "securityProfile", workload, profile },
]
