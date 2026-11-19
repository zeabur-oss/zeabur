// API tokens: their names and how long one lasts.

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
