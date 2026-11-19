// API tokens: their names and how long one lasts.
import { rules } from "@/api/rules"
import { fits } from "@/model/rules"

/** The lifetimes offered, in seconds. kube-apiserver may cut any of them short. */
export const LIFETIMES = [3600, 86_400, 7 * 86_400, 30 * 86_400, 90 * 86_400, 365 * 86_400]
export const DEFAULT_LIFETIME = 30 * 86_400

export const validName = (name: string): boolean =>
  fits(name, rules["MemberChange.createToken.name"])

export function expired(expiresAt: string | null | undefined, now: number): boolean {
  return expiresAt != null && Date.parse(expiresAt) <= now
}
