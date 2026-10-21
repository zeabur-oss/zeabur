// A service's incoming connection rules as groups by outcome, exceptions first.
import type { Schemas } from "@/api/client"

type Access = Schemas["NetworkPolicyAccess"]
type Reach = Schemas["NetworkPolicyService"]

export interface Peer {
  id: string
  name: string
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
        .map((p) => ({ id: p.service, name: p.name }))
        .toSorted((a, b) => a.name.localeCompare(b.name)),
    }))
    .filter((g) => g.peers.length > 0)
  return { open: reach.selectedBy.length === 0, groups, few: peers.length <= FEW_PEERS }
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
