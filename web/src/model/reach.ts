// A service's incoming connection rules as groups by outcome, exceptions first.
import type { Schemas } from "@/api/client"

type Access = Schemas["NetworkPolicyAccess"]
type Reach = Schemas["NetworkPolicyService"]

export interface Peer {
  id: string
  name: string
  /** The rules that decide its access, by name. */
  via: string[]
}

export interface PeerGroup {
  access: Access
  peers: Peer[]
}

export interface ReachSummary {
  /** No rule selects the service: everything may reach it. */
  open: boolean
  /** Blocked, then partly allowed, then allowed; empty groups left out. */
  groups: PeerGroup[]
  /** Every peer fits on a line: the groups list their names instead of folding them. */
  few: boolean
}

/** Up to this many peers, every name is said. */
export const FEW_PEERS = 7
/** A folded group names this many before counting the rest. */
const NAMES_SHOWN = 5

const order: Access[] = ["blocked", "limited", "allowed"]

export function reachSummary(reach: Reach, nameOf: (id: string) => string): ReachSummary {
  const peers = reach.fromServices.map((p) => ({ ...p, name: nameOf(p.service) }))
  const groups = order
    .map((access) => ({
      access,
      peers: peers
        .filter((p) => p.access === access)
        .map((p) => ({ id: p.service, name: p.name, via: p.via }))
        .toSorted((a, b) => a.name.localeCompare(b.name)),
    }))
    .filter((g) => g.peers.length > 0)
  return { open: reach.selectedBy.length === 0, groups, few: peers.length <= FEW_PEERS }
}

/** Which rule decides each peer is worth saying only when the project has more than one. */
export const showsRules = (policies: readonly string[]) => policies.length > 1

/** A few peers split by the rules that decide them, so each set of rules is said once. */
export function byRules(peers: readonly Peer[]): { via: string[]; peers: Peer[] }[] {
  const out = new Map<string, { via: string[]; peers: Peer[] }>()
  for (const peer of peers) {
    const key = peer.via.join("\n")
    const entry = out.get(key) ?? { via: peer.via, peers: [] }
    entry.peers.push(peer)
    out.set(key, entry)
  }
  return [...out.values()]
}

/** The rules, when the same ones decide every peer: said once rather than on each row. */
export function sharedRules(peers: readonly Peer[]): string[] | undefined {
  const parts = byRules(peers)
  return parts.length === 1 ? parts[0]?.via : undefined
}

/** What a group says before it is opened: its first names and how many more. */
export function namesShown(group: PeerGroup): { names: string[]; more: number } {
  if (group.access === "allowed") return { names: [], more: group.peers.length }
  const names = group.peers.slice(0, NAMES_SHOWN).map((p) => p.name)
  return { names, more: group.peers.length - names.length }
}

/** The parts of the lead sentence, by outcome: services counted, the two other sources named. */
export interface Lead {
  allowed: number
  limited: number
  blocked: number
  domains: Access
  otherProjects: Access
}

export function leadOf(reach: Reach): Lead {
  const count = (access: Access) => reach.fromServices.filter((p) => p.access === access).length
  return {
    allowed: count("allowed"),
    limited: count("limited"),
    blocked: count("blocked"),
    domains: reach.fromIngressController,
    otherProjects: reach.fromOtherNamespaces,
  }
}

export function filterPeers(peers: Peer[], query: string): Peer[] {
  const needle = query.trim().toLowerCase()
  return needle ? peers.filter((p) => p.name.toLowerCase().includes(needle)) : peers
}
