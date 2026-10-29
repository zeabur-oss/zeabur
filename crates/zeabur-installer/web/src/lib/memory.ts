import type { CatalogComponent, Plan, PlannedComponent } from "@/api"

export type Verdict = "fits" | "tight" | "over"

export interface Fit {
  verdict: Verdict
  used: number
  host: number
  /** MiB beyond the host's memory, or beyond the tight line when it is tight. */
  short: number
  /** Components to leave out, and what they free. */
  relief: { names: string[]; mib: number } | null
}

const tight = 0.8
/** Where leaving components out should bring a plan that is over or tight. */
const aim: Record<Verdict, number> = { over: 0.9, tight, fits: 1 }

export function memoryFit(plan: Plan, catalog: CatalogComponent[]): Fit {
  const { clusterMib, componentsMib, hostMib } = plan.memory
  const used = clusterMib + componentsMib
  const verdict: Verdict = used > hostMib ? "over" : used > hostMib * tight ? "tight" : "fits"
  const short =
    verdict === "over"
      ? used - hostMib
      : verdict === "tight"
        ? Math.ceil(used - hostMib * tight)
        : 0
  return {
    verdict,
    used,
    host: hostMib,
    short,
    // Squeezing just under the host's memory would leave nothing to spare.
    relief: short
      ? relief(plan.components, catalog, Math.ceil(used - hostMib * aim[verdict]))
      : null,
  }
}

/** Optional components first, then recommended ones: the smallest one that frees enough, else the
 * largest few that together do. Components another one needs are not offered. */
function relief(components: PlannedComponent[], catalog: CatalogComponent[], need: number) {
  const byId = new Map(catalog.map((c) => [c.id, c]))
  const needed = new Set(components.flatMap((c) => byId.get(c.id)?.needs ?? []))
  const mib = (c: PlannedComponent) => c.idleMemoryMib ?? 0
  const essential = (c: PlannedComponent) => byId.get(c.id)?.required ?? false
  const candidates = components.filter(
    (c) => c.why !== "dependency" && !needed.has(c.id) && !essential(c) && mib(c) > 0,
  )
  const optional = (c: PlannedComponent) => byId.get(c.id)?.default === "optional"
  for (const pool of [candidates.filter(optional), candidates]) {
    const sorted = [...pool].sort((a, b) => mib(b) - mib(a))
    const single = sorted.filter((c) => mib(c) >= need).at(-1)
    if (single) return { names: [single.displayName], mib: mib(single) }
    const picked: PlannedComponent[] = []
    for (const c of sorted) {
      picked.push(c)
      const freed = picked.reduce((sum, p) => sum + mib(p), 0)
      if (freed >= need) return { names: picked.map((p) => p.displayName), mib: freed }
    }
  }
  return null
}
