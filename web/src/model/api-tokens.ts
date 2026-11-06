// API tokens: how long one lasts, and the kubeconfig that uses one.
import type { Schemas } from "@/api/client"

/** The lifetimes offered, in seconds. kube-apiserver may cut any of them short. */
export const LIFETIMES = [3600, 86_400, 7 * 86_400, 30 * 86_400, 90 * 86_400, 365 * 86_400]
export const DEFAULT_LIFETIME = 30 * 86_400

/** Lower-case letters, digits and `-`, as a ServiceAccount's name in a token's binding. */
export function validName(name: string): boolean {
  return name.length <= 63 && /^[a-z0-9]([-a-z0-9]*[a-z0-9])?$/.test(name)
}

/** The role a new token gets: the one picked while it is open in `cluster`'s scope, else the first
 * open one. */
export function roleFor(
  choices: readonly Schemas.RoleChoice[],
  cluster: boolean,
  picked: Schemas.ProjectRole | undefined,
): Schemas.ProjectRole | undefined {
  let first: Schemas.ProjectRole | undefined
  for (const choice of choices) {
    if (choice.cluster !== cluster || choice.permission === "denied") continue
    if (choice.role === picked) return picked
    first ??= choice.role
  }
  return first
}

/** Why a token for every namespace is closed; nothing when some role is open there. */
export function clusterRefusal(
  choices: readonly Schemas.RoleChoice[],
): Schemas.ChoiceRefusal | undefined {
  let refusal: Schemas.ChoiceRefusal | undefined
  for (const choice of choices) {
    if (!choice.cluster) continue
    if (choice.permission !== "denied") return undefined
    refusal ??= choice.refusal ?? undefined
  }
  return refusal ?? "notAdmin"
}

export function expired(expiresAt: string | null | undefined, now: number): boolean {
  return expiresAt != null && Date.parse(expiresAt) <= now
}

/** A kubeconfig for the token with `server` blank: Zeabur knows kube-apiserver only by its
 * in-cluster address, so the person fills in the one they reach it on. */
export function kubeconfig(issued: Schemas.IssuedTokenView): string {
  const ca = issued.ca ? `\n      certificate-authority-data: ${btoa(issued.ca)}` : ""
  return `apiVersion: v1
kind: Config
clusters:
  - name: zeabur
    cluster:
      server: ""${ca}
users:
  - name: ${issued.name}
    user:
      token: ${issued.token}
contexts:
  - name: ${issued.name}
    context:
      cluster: zeabur
      user: ${issued.name}
      namespace: ${issued.namespace}
current-context: ${issued.name}
`
}
