// A plan of many changes, grouped: edits by object and part as field, before and after; restarts
// and scaling of many services by what happens to each.
import type { Schemas } from "@/api/client"
import type { Translate } from "@/i18n"
import { kindName } from "@/i18n/kind-name"
import type { Dimension } from "@/lib/amount"
import { plainAmount } from "@/lib/quantity"
import { partOrder } from "@/model/draft"
import type { PodsOf } from "@/model/risk"
import type { DraftPart } from "@/routes/search"

type Change = Schemas["Change"]
type WorkloadRef = Schemas["WorkloadRef"]
type Plan = Pick<Schemas["PlanResponse"], "objects"> & {
  preview?: Pick<Schemas["Preview"], "rollouts">
}

/** Up to this many edits, or changes, read as sentences. */
export const FEW = 5

export type RowKind = "removed" | "changed" | "added"

export interface FieldRow {
  kind: RowKind
  /** A label in words ("Reserved CPU"). */
  label?: string
  /** A name one could paste (a variable, a key, a path). */
  name?: string
  before?: string
  after?: string
}

type GroupPart = DraftPart | "other"

export interface EditGroup {
  key: string
  /** The object, when the plan changes more than one. */
  object?: string
  part: GroupPart
  /** The container, when the object's edits touch more than one. */
  container?: string
  rows: FieldRow[]
}

const workloadKinds = new Set(["Deployment", "StatefulSet", "DaemonSet", "Job", "CronJob"])
const networkKinds = new Set(["Ingress", "HTTPRoute", "Service", "NetworkPolicy", "Gateway"])

/** Splits a dotted path, keeping `[name=a.b]` whole. */
export function segments(path: string): string[] {
  const out: string[] = []
  let depth = 0
  let current = ""
  for (const char of path) {
    if (char === "[") depth++
    if (char === "]") depth--
    if (char === "." && depth === 0) {
      out.push(current)
      current = ""
    } else current += char
  }
  out.push(current)
  return out
}

/** `env[name=FOO]` → `FOO`. */
const keyOf = (segment: string) => /\[[^=\]]+=(.*)\]$/.exec(segment)?.[1]

interface Slot {
  part: GroupPart
  container?: string
  label?: string
  name?: string
  /** The values are CPU or memory in Kubernetes' units; the rows show them in plain ones. */
  amount?: Dimension
  /** A keyed list entry whose leaves make one row. */
  entry?: string
  /** The leaf below the entry, `value` or `readOnly`. */
  leaf?: string
}

function resourceSlot(
  t: Translate,
  container: string | undefined,
  [bound, resource]: string[],
  name: string,
): Slot {
  const label = resourceLabel(t, bound, resource)
  if (!label) return { part: "compute", container, name }
  return { part: "compute", container, label, amount: resource === "cpu" ? "cpu" : "bytes" }
}

function slotOf(t: Translate, kind: string, path: string): Slot | undefined {
  const segs = segments(path)
  if (workloadKinds.has(kind)) {
    if (path === "spec.replicas") return { part: "compute", label: t("form.replicas") }
    if (path.startsWith("spec.template.metadata.annotations.") && path.endsWith("restartedAt"))
      return undefined
    const spec = path.startsWith("spec.template.spec.")
      ? segs.slice(3)
      : path.startsWith("spec.jobTemplate.spec.template.spec.")
        ? segs.slice(5)
        : undefined
    if (spec) {
      const [first = "", ...rest] = spec
      if (first.startsWith("volumes[")) return entrySlot("storage", first, rest)
      if (first.startsWith("containers[") || first.startsWith("initContainers[")) {
        const container = keyOf(first)
        const [what = "", ...below] = rest
        if (what.startsWith("env[")) return { ...entrySlot("variables", what, below), container }
        if (what.startsWith("volumeMounts["))
          return { ...entrySlot("storage", what, below), container }
        if (what === "resources") return resourceSlot(t, container, below, rest.join("."))
        const probe = /^(readiness|liveness|startup)Probe$/.exec(what)?.[1] as
          | Schemas["ProbeKind"]
          | undefined
        if (probe)
          return {
            part: "health-checks",
            container,
            label: t(`form.probe.${probe}`),
            name: below.join(".") || undefined,
          }
        return { part: "other", container, name: rest.join(".") }
      }
    }
    if (segs[0] === "spec" && segs[1]?.startsWith("volumeClaimTemplates"))
      return { part: "storage", name: segs.slice(1).join(".") }
    return { part: "other", name: path }
  }
  if (kind === "ConfigMap" || kind === "Secret") {
    if (segs[0] === "data" || segs[0] === "stringData" || segs[0] === "binaryData")
      return { part: "variables", name: segs.slice(1).join(".") }
    return { part: "other", name: path }
  }
  if (kind === "PersistentVolumeClaim")
    return path === "spec.resources.requests.storage"
      ? { part: "storage", label: t("plan.field.size"), amount: "bytes" }
      : { part: "storage", name: path }
  if (networkKinds.has(kind)) return { part: "networking", name: path }
  return { part: "other", name: path }
}

function partOfKind(kind: string): GroupPart {
  if (kind === "ConfigMap" || kind === "Secret") return "variables"
  if (kind === "PersistentVolumeClaim") return "storage"
  return networkKinds.has(kind) ? "networking" : "other"
}

function entrySlot(part: GroupPart, segment: string, below: string[]): Slot {
  return { part, entry: keyOf(segment) ?? segment, leaf: below.join(".") }
}

function resourceLabel(t: Translate, bound?: string, resource?: string): string | undefined {
  if (bound === "requests" && resource === "cpu") return t("form.cpuRequest")
  if (bound === "requests" && resource === "memory") return t("form.memoryRequest")
  if (bound === "limits" && resource === "cpu") return t("form.cpuLimit")
  if (bound === "limits" && resource === "memory") return t("form.memoryLimit")
  return undefined
}

const kindOf = (before?: string | null, after?: string | null): RowKind =>
  before === undefined || before === null
    ? "added"
    : after === undefined || after === null
      ? "removed"
      : "changed"

const rank: Record<RowKind, number> = { removed: 0, changed: 1, added: 2 }
const partRank = (part: GroupPart) => {
  const at = partOrder.indexOf(part as DraftPart)
  return at === -1 ? partOrder.length : at
}

/** The plan's field changes as rows, by object then part (in the order of the tabs), each with
 * removed rows first, then changed, then added. */
export function editGroups(t: Translate, plan: Pick<Plan, "objects">): EditGroup[] {
  const named = plan.objects.length > 1
  const groups: EditGroup[] = []
  for (const object of plan.objects) {
    const { kind, name } = object.target
    const label = named ? `${kindName(t, kind)} ${name}` : undefined
    const created = !object.beforeYaml && !object.before
    const deleted = object.write.type === "delete"
    if (created || deleted) {
      groups.push({
        key: `${kind}/${name}`,
        object: label,
        part: partOfKind(kind),
        rows: [
          {
            kind: created ? "added" : "removed",
            label: kind,
            name,
            after: created ? t("plan.create") : undefined,
            before: deleted ? t("plan.exists") : undefined,
          },
        ],
      })
      continue
    }
    interface Bucket {
      group: EditGroup
      /** Rows, and the entries (`env[name=FOO]`) whose leaves become rows at the end. */
      items: (FieldRow | string)[]
      entries: Map<string, Leaves>
    }
    const buckets = new Map<string, Bucket>()
    const placed = object.changes.flatMap((change) => {
      const slot = slotOf(t, kind, change.path)
      return slot ? [{ slot, change }] : []
    })
    const containers = new Set(placed.map((p) => p.slot.container).filter(Boolean))
    for (const { slot, change } of placed) {
      const container = containers.size > 1 ? slot.container : undefined
      const show = (value?: string | null) =>
        value === undefined || value === null || !slot.amount
          ? (value ?? undefined)
          : plainAmount(t, value, slot.amount)
      const key = `${kind}/${name}/${slot.part}/${container ?? ""}`
      const bucket = buckets.get(key) ?? {
        group: { key, object: label, part: slot.part, container, rows: [] },
        items: [],
        entries: new Map<string, Leaves>(),
      }
      buckets.set(key, bucket)
      if (slot.entry === undefined) {
        bucket.items.push({
          kind: kindOf(change.before, change.after),
          label: slot.label,
          name: slot.name,
          before: show(change.before),
          after: show(change.after),
        })
        continue
      }
      const leaves = bucket.entries.get(slot.entry)
      if (leaves) leaves.set(slot.leaf ?? "", change)
      else {
        bucket.entries.set(slot.entry, new Map([[slot.leaf ?? "", change]]))
        bucket.items.push(slot.entry)
      }
    }
    for (const { group, items, entries } of buckets.values()) {
      group.rows = items
        .flatMap((item) =>
          typeof item === "string" ? entryRows(item, entries.get(item) ?? new Map()) : [item],
        )
        .sort((a, b) => rank[a.kind] - rank[b.kind])
      groups.push(group)
    }
  }
  const objectOrder = new Map<string | undefined, number>()
  for (const group of groups)
    if (!objectOrder.has(group.object)) objectOrder.set(group.object, objectOrder.size)
  return groups
    .map((group, index) => ({ group, index }))
    .sort(
      (a, b) =>
        (objectOrder.get(a.group.object) ?? 0) - (objectOrder.get(b.group.object) ?? 0) ||
        partRank(a.group.part) - partRank(b.group.part) ||
        a.index - b.index,
    )
    .map(({ group }) => group)
}

type Leaves = Map<string, Schemas["FieldChange"]>

/** One row for an entry of a keyed list (`env[name=FOO]`): its value, or one row per other
 * field when it has no value (`valueFrom`, a mount's `readOnly`). */
function entryRows(entry: string, leaves: Leaves): FieldRow[] {
  const keyLeaf = leaves.get("name") ?? leaves.get("mountPath")
  const value = leaves.get("value")
  const others = [...leaves].filter(
    ([leaf]) => leaf !== "name" && leaf !== "mountPath" && leaf !== "value",
  )
  const entryKind = keyLeaf ? kindOf(keyLeaf.before, keyLeaf.after) : undefined
  if (value || others.length === 0) {
    const source = value ?? keyLeaf
    const blank = (side?: string | null) =>
      value ? (side ?? undefined) : side === undefined || side === null ? undefined : ""
    return [
      {
        kind: entryKind ?? kindOf(source?.before, source?.after),
        name: entry,
        before: blank(source?.before),
        after: blank(source?.after),
      },
      ...others.map(([leaf, change]) => leafRow(entry, leaf, change)),
    ]
  }
  return others.map(([leaf, change]) => leafRow(entry, leaf, change))
}

const leafRow = (entry: string, leaf: string, change: Schemas["FieldChange"]): FieldRow => ({
  kind: kindOf(change.before, change.after),
  name: `${entry} ${leaf}`,
  before: change.before ?? undefined,
  after: change.after ?? undefined,
})

export const countRows = (groups: readonly EditGroup[]) => {
  const counts: Record<RowKind, number> = { removed: 0, changed: 0, added: 0 }
  for (const group of groups) for (const row of group.rows) counts[row.kind]++
  return counts
}

// ── Many services ─────────────────────────────────────────────────────────────────────────

/** What a restart or scale does to one service, exceptions first. */
type Outcome = "stops" | "down" | "notReady" | "single" | "starts" | "up"

const outcomeOrder: Outcome[] = ["stops", "down", "notReady", "single", "starts", "up"]
const exceptions = new Set<Outcome>(["stops", "down", "notReady"])

interface OutcomeRow {
  workload: WorkloadRef
  /** "2/3" ready of desired for a restart, "3 → 0" for a scale. */
  replicas: string
  /** What sets this one apart from the rest of its group; none when nothing does. */
  effect?: string
}

export interface OutcomeGroup {
  outcome: Outcome
  rows: OutcomeRow[]
  /** Exceptions are open; the rest wait behind their count. */
  open: boolean
}

/** Whether the ask is restarts or scaling only, which group by outcome rather than by field. */
export const isBulk = (changes: readonly Change[]) =>
  changes.length > 0 && changes.every((c) => c.op === "restart" || c.op === "scale")

/** A workload's persistent mounts, as the project document reports them. */
export type MountsOf = (workload: WorkloadRef) => string[]

export function outcomeGroups(
  t: Translate,
  changes: readonly Change[],
  plan: Plan,
  podsOf: PodsOf,
  mountsOf: MountsOf = () => [],
): OutcomeGroup[] {
  const rows = new Map<Outcome, OutcomeRow[]>()
  const add = (outcome: Outcome, row: OutcomeRow) =>
    rows.set(outcome, [...(rows.get(outcome) ?? []), row])
  for (const change of changes) {
    if (change.op !== "restart" && change.op !== "scale") continue
    const { workload } = change
    const rollout = plan.preview?.rollouts.find(
      (r) => r.workload.kind === workload.kind && r.workload.name === workload.name,
    )
    const pods = podsOf(workload)
    const effects: string[] = []
    if (rollout?.strategy === "Recreate") effects.push(t("plan.effect.recreate"))
    if (rollout?.strategy === "OnDelete") effects.push(t("plan.effect.onDelete"))
    if (mountsOf(workload).length > 0) effects.push(t("plan.effect.dataStays"))
    const effect = effects.length > 0 ? effects.join(t("common.sep")) : undefined
    if (change.op === "scale") {
      const from = pods?.desired ?? rollout?.fromReplicas ?? undefined
      const replicas = from === undefined ? String(change.replicas) : `${from} → ${change.replicas}`
      const outcome: Outcome =
        change.replicas === 0 ? "stops" : from === 0 ? "starts" : rollout?.downtime ? "down" : "up"
      add(outcome, { workload, replicas, effect })
      continue
    }
    const ready = pods?.ready ?? 0
    const replicas = pods ? `${pods.ready}/${pods.desired}` : "—"
    const outcome: Outcome = rollout?.downtime
      ? "down"
      : pods && ready === 0
        ? "notReady"
        : ready === 1 || pods?.desired === 1
          ? "single"
          : "up"
    add(outcome, { workload, replicas, effect })
  }
  const present = outcomeOrder.filter((o) => rows.has(o))
  return present.map((outcome) => ({
    outcome,
    rows: rows.get(outcome) ?? [],
    open: present.length === 1 || exceptions.has(outcome),
  }))
}

/** At most `shown` names and how many more, as one list; `rest` holds the names left out. */
export function bounded(
  t: Translate,
  names: readonly string[],
  shown = 3,
): { text: string; rest: string[] } {
  if (names.length <= shown + 1) return { text: t("common.list", [...names]), rest: [] }
  const rest = names.slice(shown)
  return {
    text: t("common.list", [...names.slice(0, shown), t("common.more", rest.length)]),
    rest,
  }
}
