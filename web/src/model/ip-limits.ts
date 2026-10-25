// IP limits of a service: the addresses that may open its domains (a Traefik `ipAllowList`
// middleware per Ingress) and the addresses its pods may connect to (a NetworkPolicy).
import type { Schemas } from "@/api/client"
import type { Translate } from "@/i18n"

/** The NetworkPolicy that limits where the pods of a workload connect. */
export const egressPolicyName = (workload: string) => `${workload}-egress-limit`

/** Entries of a list typed one per line, or separated by commas or spaces. */
export function splitAddresses(text: string): string[] {
  return [...new Set(text.split(/[\s,]+/).filter(Boolean))]
}

const V4 = /^(25[0-5]|2[0-4]\d|1?\d?\d)(\.(25[0-5]|2[0-4]\d|1?\d?\d)){3}$/
const V6 = /^[0-9a-f:.]*:[0-9a-f:.]*$/i

/** Whether `entry` reads as an IP address or a CIDR range. The plan checks it again. */
export function isAddress(entry: string): boolean {
  const [address = "", prefix, ...rest] = entry.split("/")
  if (rest.length > 0) return false
  const v4 = V4.test(address)
  if (!v4 && !(V6.test(address) && address.length >= 2)) return false
  if (prefix === undefined) return true
  return /^\d{1,3}$/.test(prefix) && Number(prefix) <= (v4 ? 32 : 128)
}

/** The entries of `cidrs` that are neither an IP address nor a CIDR range. */
export const badAddresses = (cidrs: string[]) => cidrs.filter((c) => !isAddress(c))

/** The entries under `key:` in a Kubernetes object's YAML, as a block or a flow list. */
function listUnder(yaml: string | undefined, key: string): string[] {
  const lines = (yaml ?? "").split("\n")
  const at = lines.findIndex((line) => line.trim().startsWith(`${key}:`))
  const head = lines[at]
  if (head === undefined) return []
  const inline = head.slice(head.indexOf(":") + 1).trim()
  if (inline.startsWith("[")) return splitAddresses(inline.replace(/[[\]"']/g, " "))
  const indent = head.length - head.trimStart().length
  const out: string[] = []
  for (const line of lines.slice(at + 1)) {
    const item = /^(\s*)-\s+(.+)$/.exec(line)
    if (!item || (item[1] ?? "").length < indent) break
    out.push((item[2] ?? "").replace(/^["']|["']$/g, ""))
  }
  return out
}

/** The addresses a Traefik `ipAllowList` Middleware lets through. */
export const sourceRangeOf = (yaml: string | undefined) => listUnder(yaml, "sourceRange")

/** The `ipBlock` ranges a NetworkPolicy allows. */
export function blocksOf(yaml: string | undefined): string[] {
  return [...(yaml ?? "").matchAll(/^\s*cidr:\s*["']?([^\s"']+)/gm)].map((m) => m[1] ?? "")
}

export type Kind = "in" | "out"

/** What the addresses of a limit are now, for the row that says it. */
export type Verdict =
  | { is: "noLimit" | "limited" | "some" | "rules" }
  | { is: "only"; addresses: string[] }

/** The domains: `loaded` holds the addresses of each limited Ingress, `undefined` until read. */
export function incomingVerdict(
  ingresses: number,
  loaded: readonly (string[] | undefined)[],
): Verdict {
  if (loaded.length === 0) return { is: "noLimit" }
  const [first = []] = loaded
  const known = loaded.every((set) => set !== undefined)
  if (!known) return { is: "limited" }
  const same = loaded.length === ingresses && loaded.every((set) => set?.join() === first.join())
  return same ? { is: "only", addresses: first } : { is: "some" }
}

/** The pods: whether Zeabur's policy selects them, whether another policy limits their egress,
 * and the ranges of Zeabur's policy once read. */
export function outgoingVerdict(
  ours: boolean,
  others: boolean,
  blocks: string[] | undefined,
): Verdict {
  if (!ours) return { is: others ? "rules" : "noLimit" }
  return blocks ? { is: "only", addresses: blocks } : { is: "limited" }
}

export interface Was {
  on: boolean
  /** Every Ingress has these addresses now (domains). */
  same: boolean
  cidrs: string[]
}

/** Why a form's addresses cannot be reviewed yet. */
export function problemOf(kind: Kind, on: boolean, cidrs: string[]): "bad" | "none" | undefined {
  if (!on) return undefined
  if (badAddresses(cidrs).length > 0) return "bad"
  return kind === "in" && cidrs.length === 0 ? "none" : undefined
}

/** Whether the form asks for what is already there. */
export function isUnchanged(was: Was, on: boolean, cidrs: string[]): boolean {
  if (on !== was.on) return false
  return !on || (was.same && cidrs.join() === was.cidrs.join())
}

/** The changes of a form: one for the pods, one for each Ingress that must change. */
export function limitChanges(
  kind: Kind,
  workload: Schemas["WorkloadRef"],
  ingresses: readonly { name: string; limited: boolean }[],
  on: boolean,
  cidrs: string[],
): Schemas["Change"][] {
  const set = on ? cidrs : []
  if (kind === "out") return [{ op: "egressLimit", workload, enabled: on, cidrs: set }]
  return ingresses
    .filter((r) => on || r.limited)
    .map((r) => ({
      op: "ipAllow",
      route: { kind: "Ingress", name: r.name },
      enabled: on,
      cidrs: set,
    }))
}

/** What a limit of the plan does, in a sentence. */
export function limitSentence(
  t: Translate,
  change: Extract<Schemas["Change"], { op: "ipAllow" | "egressLimit" }>,
): string {
  const list = t("common.list", change.cidrs ?? [])
  if (change.op === "ipAllow")
    return change.enabled
      ? t("confirm.ipAllow", change.route.name, list)
      : t("confirm.ipAllowOff", change.route.name)
  const name = change.workload.name
  if (!change.enabled) return t("confirm.egressLimitOff", name)
  return list ? t("confirm.egressLimit", name, list) : t("confirm.egressLimitNone", name)
}
