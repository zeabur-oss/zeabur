// API tokens: how long one lasts, and the kubeconfig that uses one.
import type { Schemas } from "@/api/client"

/** The lifetimes offered, in seconds. kube-apiserver may cut any of them short. */
export const LIFETIMES = [3600, 86_400, 7 * 86_400, 30 * 86_400, 90 * 86_400, 365 * 86_400]
export const DEFAULT_LIFETIME = 30 * 86_400

/** Lower-case letters, digits and `-`, as a ServiceAccount's name. */
export function validName(name: string): boolean {
  return name.length <= 63 && /^[a-z0-9]([-a-z0-9]*[a-z0-9])?$/.test(name)
}

export function expired(expiresAt: string | null | undefined, now: number): boolean {
  return expiresAt != null && Date.parse(expiresAt) <= now
}

/** The namespace a new kubeconfig starts in: the token's project when it has exactly one. */
export function startNamespace(projects: readonly Schemas.ProjectGrant[]): string | undefined {
  return projects.length === 1 ? projects[0]?.project : undefined
}

/** A kubeconfig for the token with `server` blank: Zeabur knows kube-apiserver only by its
 * in-cluster address, so the person fills in the one they reach it on. */
export function kubeconfig(issued: Schemas.IssuedTokenView, namespace?: string): string {
  // JSON strings are YAML strings, so a name such as `null` stays a name.
  const q = JSON.stringify
  const name = q(issued.name)
  const ca = issued.ca ? `\n      certificate-authority-data: ${btoa(issued.ca)}` : ""
  const ns = namespace ? `\n      namespace: ${q(namespace)}` : ""
  return `apiVersion: v1
kind: Config
clusters:
  - name: zeabur
    cluster:
      server: ""${ca}
users:
  - name: ${name}
    user:
      token: ${q(issued.token)}
contexts:
  - name: ${name}
    context:
      cluster: zeabur
      user: ${name}${ns}
current-context: ${name}
`
}
