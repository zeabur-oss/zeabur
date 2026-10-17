// The Architecture tab's automatic layered layout, left to right: Domains → Web (exposed by an
// Ingress) → Internal (behind a Service) → Workers & jobs → Data. Within a layer, nodes are ordered
// by the barycentre of their neighbours (a few down and up sweeps; Sugiyama, Tagawa and Toda 1981)
// to reduce crossings. Deterministic: the same project always gives the same coordinates.
import type { Edge } from "@/model/connections"
import {
  domainsOf,
  isHealthy,
  roleOf,
  serviceId,
  statusOf,
  volumesOf,
  type Role,
  type Service,
} from "@/model/service"

const CARD_W = 224
const CARD_H = 112
const DRAWER_H = 30
const DOMAIN_W = 220
const DOMAIN_H = 40
const GAP_X = 84
const GAP_Y = 36
const FRAME_PAD = 18
const FRAME_HEAD = 30

export type Lane = "domains" | "web" | "internal" | "workers" | "data"
const laneOf: Record<Role, Lane> = {
  web: "web",
  internal: "internal",
  worker: "workers",
  job: "workers",
  data: "data",
}
const laneOrder: Lane[] = ["domains", "web", "internal", "workers", "data"]

export interface LaidNode {
  id: string
  lane: Lane
  x: number
  y: number
  w: number
  h: number
  service?: Service
  host?: string
  /** A domain whose Ingress lists it under TLS. */
  https?: boolean
}

export interface LaidEdge extends Edge {
  path: string
  mid: { x: number; y: number }
}

interface Frame {
  lane: Lane
  x: number
  y: number
  w: number
  h: number
  ready: number
  total: number
}

export interface Layout {
  nodes: LaidNode[]
  edges: LaidEdge[]
  frames: Frame[]
  width: number
  height: number
}

const real = (id: string) => !id.startsWith("dummy:")

function spread(list: string[], id: string): number {
  const i = list.indexOf(id)
  return (i - (list.length - 1) / 2) * 12
}

export function layout(services: Service[], allEdges: Edge[]): Layout {
  const nodes = new Map<string, LaidNode>()
  for (const service of services) {
    const id = serviceId(service)
    nodes.set(id, {
      id,
      lane: laneOf[roleOf(service)],
      x: 0,
      y: 0,
      w: CARD_W,
      h: CARD_H + volumesOf(service).length * DRAWER_H,
      service,
    })
    for (const { host, https } of domainsOf(service)) {
      const domainId = `domain:${host}`
      if (!nodes.has(domainId))
        nodes.set(domainId, {
          id: domainId,
          lane: "domains",
          x: 0,
          y: 0,
          w: DOMAIN_W,
          h: DOMAIN_H,
          host,
          https,
        })
    }
  }

  const edges = allEdges.filter((e) => nodes.has(e.from) && nodes.has(e.to))
  const lanes = laneOrder
    .map((lane) => ({
      lane,
      ids: [...nodes.values()]
        .filter((n) => n.lane === lane)
        .map((n) => n.id)
        .toSorted(),
    }))
    .filter((l) => l.ids.length > 0)

  const laneIndex = new Map<string, number>()
  lanes.forEach((l, i) => l.ids.forEach((id) => laneIndex.set(id, i)))

  // An edge spanning more than one lane gets a dummy point in each lane it crosses (Sugiyama):
  // the dummies take part in the ordering and are placed in the gaps between cards, so the edge
  // runs between cards instead of behind them.
  const neighbours = new Map<string, string[]>()
  const link = (a: string, b: string) => {
    neighbours.set(a, [...(neighbours.get(a) ?? []), b])
    neighbours.set(b, [...(neighbours.get(b) ?? []), a])
  }
  const dummies = new Map<string, string[]>()
  for (const edge of edges) {
    const from = laneIndex.get(edge.from) ?? 0
    const to = laneIndex.get(edge.to) ?? 0
    if (to - from <= 1) {
      link(edge.from, edge.to)
      continue
    }
    const chain: string[] = []
    for (let li = from + 1; li < to; li++) {
      const id = `dummy:${edge.id}:${li}`
      chain.push(id)
      lanes[li]?.ids.push(id)
      laneIndex.set(id, li)
    }
    dummies.set(edge.id, chain)
    ;[edge.from, ...chain, edge.to].reduce((a, b) => {
      link(a, b)
      return b
    })
  }
  const passes = new Map<string, { y: number; left: number; right: number }>()

  // Barycentre sweeps: down (look left), then up (look right).
  const position = () => {
    const pos = new Map<string, number>()
    for (const l of lanes) l.ids.forEach((id, i) => pos.set(id, (i + 0.5) / l.ids.length))
    return pos
  }
  for (let round = 0; round < 4; round++) {
    for (const direction of [-1, 1]) {
      const order = direction === -1 ? lanes.keys() : [...lanes.keys()].toReversed()
      for (const li of order) {
        const pos = position()
        const lane = lanes[li]
        if (!lane) continue
        const score = (id: string) => {
          const ns = (neighbours.get(id) ?? []).filter((n) => {
            const nl = laneIndex.get(n) ?? li
            return direction === -1 ? nl < li : nl > li
          })
          if (ns.length === 0) return pos.get(id) ?? 0
          return ns.reduce((sum, n) => sum + (pos.get(n) ?? 0), 0) / ns.length
        }
        lane.ids = lane.ids.toSorted((a, b) => score(a) - score(b) || a.localeCompare(b))
      }
    }
  }

  // Coordinates: lanes left to right, each column centred on the tallest.
  const columnHeight = (all: string[]) => {
    const ids = all.filter(real)
    return ids.reduce((sum, id) => sum + (nodes.get(id)?.h ?? 0), 0) + GAP_Y * (ids.length - 1)
  }
  const tallest = Math.max(...lanes.map((l) => columnHeight(l.ids)))
  let x = 0
  const frames: Frame[] = []
  for (const l of lanes) {
    const width = Math.max(...l.ids.filter(real).map((id) => nodes.get(id)?.w ?? 0))
    let y = (tallest - columnHeight(l.ids)) / 2
    for (const id of l.ids) {
      const node = nodes.get(id)
      if (!node) continue
      node.x = x + (width - node.w) / 2
      node.y = y
      y += node.h + GAP_Y
    }
    // Dummies: spread evenly over the gap they were ordered into.
    const groups: { before?: LaidNode; after?: LaidNode; ids: string[] }[] = []
    let previous: LaidNode | undefined
    let pending: string[] = []
    for (const id of l.ids) {
      if (!real(id)) {
        pending.push(id)
        continue
      }
      const node = nodes.get(id)
      if (pending.length) groups.push({ before: previous, after: node, ids: pending })
      pending = []
      previous = node
    }
    if (pending.length) groups.push({ before: previous, ids: pending })
    for (const group of groups) {
      const top = group.before ? group.before.y + group.before.h : (group.after?.y ?? 0) - 56
      const bottom = group.after ? group.after.y : top + 56
      group.ids.forEach((id, i) => {
        const gy = top + ((i + 1) * (bottom - top)) / (group.ids.length + 1)
        passes.set(id, { y: gy, left: x - FRAME_PAD, right: x + width + FRAME_PAD })
      })
    }
    const members = l.ids.map((id) => nodes.get(id)).filter((n): n is LaidNode => !!n)
    const top = Math.min(...members.map((n) => n.y))
    const bottom = Math.max(...members.map((n) => n.y + n.h))
    const laneServices = members.flatMap((n) => (n.service ? [n.service] : []))
    frames.push({
      lane: l.lane,
      x: x - FRAME_PAD,
      y: top - FRAME_PAD - FRAME_HEAD,
      w: width + FRAME_PAD * 2,
      h: bottom - top + FRAME_PAD * 2 + FRAME_HEAD,
      ready: laneServices.filter((s) => isHealthy(statusOf(s))).length,
      total: l.lane === "domains" ? members.length : laneServices.length,
    })
    x += width + GAP_X
  }

  // Edges: horizontal Bézier curves, end points spread over each card's header.
  const anchorY = (node: LaidNode) => node.y + Math.min(node.h, CARD_H) / 2
  const outs = new Map<string, string[]>()
  const ins = new Map<string, string[]>()
  for (const edge of edges) {
    outs.set(edge.from, [...(outs.get(edge.from) ?? []), edge.id])
    ins.set(edge.to, [...(ins.get(edge.to) ?? []), edge.id])
  }
  const sortByOther = (list: string[], other: "from" | "to") =>
    list.toSorted((a, b) => {
      const ea = edges.find((e) => e.id === a)
      const eb = edges.find((e) => e.id === b)
      return (nodes.get(ea?.[other] ?? "")?.y ?? 0) - (nodes.get(eb?.[other] ?? "")?.y ?? 0)
    })
  const laid: LaidEdge[] = []
  for (const edge of edges) {
    const a = nodes.get(edge.from)
    const b = nodes.get(edge.to)
    if (!a || !b) continue
    const x1 = a.x + a.w
    const y1 = anchorY(a) + spread(sortByOther(outs.get(edge.from) ?? [], "to"), edge.id)
    const forward = b.x > a.x + a.w
    const x2 = forward ? b.x : b.x + b.w
    const y2 = anchorY(b) + spread(sortByOther(ins.get(edge.to) ?? [], "from"), edge.id)
    if (!forward) {
      // Same lane: a loop out to the right.
      const bend = 70
      laid.push({
        ...edge,
        path: `M${x1},${y1} C${x1 + bend},${y1} ${x2 + bend},${y2} ${x2},${y2}`,
        mid: { x: x1 + bend * 0.75, y: (y1 + y2) / 2 },
      })
      continue
    }
    const points: [number, number][] = [[x1, y1]]
    for (const id of dummies.get(edge.id) ?? []) {
      const pass = passes.get(id)
      if (!pass) continue
      points.push([pass.left, pass.y], [pass.right, pass.y])
    }
    points.push([x2, y2])
    let path = `M${x1},${y1}`
    for (let i = 1; i < points.length; i++) {
      const [px, py] = points[i - 1] ?? [0, 0]
      const [qx, qy] = points[i] ?? [0, 0]
      const straight = i % 2 === 0 && i < points.length - 1
      if (straight) path += ` L${qx},${qy}`
      else {
        const dx = Math.max(24, (qx - px) / 2)
        path += ` C${px + dx},${py} ${qx - dx},${qy} ${qx},${qy}`
      }
    }
    const middle = Math.floor(points.length / 2)
    const [ma, mb] = [points[middle - 1] ?? [x1, y1], points[middle] ?? [x2, y2]]
    laid.push({ ...edge, path, mid: { x: (ma[0] + mb[0]) / 2, y: (ma[1] + mb[1]) / 2 } })
  }

  const all = [...nodes.values()]
  return {
    nodes: all,
    edges: laid,
    frames,
    width: Math.max(...frames.map((f) => f.x + f.w)) + FRAME_PAD,
    height: Math.max(...frames.map((f) => f.y + f.h), ...all.map((n) => n.y + n.h)),
  }
}

export function bounds(l: Layout) {
  const xs = [...l.frames.map((f) => f.x), ...l.nodes.map((n) => n.x)]
  const ys = [...l.frames.map((f) => f.y), ...l.nodes.map((n) => n.y)]
  const xe = [...l.frames.map((f) => f.x + f.w), ...l.nodes.map((n) => n.x + n.w)]
  const ye = [...l.frames.map((f) => f.y + f.h), ...l.nodes.map((n) => n.y + n.h)]
  return {
    minX: Math.min(...xs),
    minY: Math.min(...ys),
    maxX: Math.max(...xe),
    maxY: Math.max(...ye),
  }
}
