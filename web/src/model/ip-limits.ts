// IP limits of a service: the addresses that may open its domains (a Traefik `ipAllowList`
// middleware per Ingress) and the addresses its pods may connect to (a NetworkPolicy).

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
