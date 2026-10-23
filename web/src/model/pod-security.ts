// A service's security profile, Strict or Compatible: the Pod Security Standards level its pods
// meet (`restricted`, `baseline`), and what choosing one does to the project's enforced level.
// Mirrors crates/zeabur-plan/src/pod_security.rs.
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

/** Compatible lowers a project that enforces restricted; any other level already allows it, and
 * a project with no level is left to the cluster's default. */
export const lowersProject = (profile: Profile, enforced: Level | null | undefined) =>
  profile === "baseline" && enforced === "restricted"

/** The changes that make an existing workload `profile`, the project's level with them. */
export function profileChanges(
  workload: Schemas["WorkloadRef"],
  profile: Profile,
  enforced: Level | null | undefined,
): Change[] {
  const changes: Change[] = [{ op: "securityProfile", workload, profile }]
  if (lowersProject(profile, enforced)) changes.push({ op: "podSecurity", level: "baseline" })
  return changes
}
