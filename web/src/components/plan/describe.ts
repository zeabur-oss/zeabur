import type { Schemas } from "@/api/client"
import type { Locale, Translate } from "@/i18n"
import {
  bounded,
  countRows,
  editGroups,
  FEW,
  isBulk,
  outcomeGroups,
  type EditGroup,
  type MountsOf,
  type OutcomeGroup,
} from "@/components/plan/groups"
import { editCount, partOf } from "@/model/draft"
import { problemText } from "@/model/problem"
import { isolates, undoOf, type PodsOf, type Reason } from "@/model/risk"
import { imageTag } from "@/model/service"

type Change = Schemas["Change"]
type WorkloadRef = Schemas["WorkloadRef"]
type Plan = Pick<Schemas["PlanResponse"], "objects" | "impact" | "warnings"> & {
  preview?: Schemas["Preview"]
}

const workloadKinds = new Set(["Deployment", "StatefulSet", "DaemonSet", "Job", "CronJob"])

/** A workload the plan creates starts its replicas; it replaces none. */
const created = (o: Plan["objects"][number]) => o.write.type === "create"

export function replaced(plan: Plan): WorkloadRef[] {
  return plan.objects
    .filter(
      (o) =>
        workloadKinds.has(o.target.kind) &&
        !created(o) &&
        o.changes.some((c) => c.path === "spec.template" || c.path.startsWith("spec.template.")),
    )
    .map((o) => ({ kind: o.target.kind as WorkloadRef["kind"], name: o.target.name }))
}

/** Whether the plan restarts a service by changing its own settings or a ConfigMap or Secret it
 * reads, so the restart rule applies. A plain restart is left out. */
export function restartRuleApplies(plan: Plan): boolean {
  const own = plan.objects.some(
    (o) =>
      workloadKinds.has(o.target.kind) &&
      !created(o) &&
      o.changes.some((c) => c.path.startsWith("spec.template.") && !c.path.endsWith("restartedAt")),
  )
  return (
    own || plan.warnings.some((w) => w.code === "restartNeeded" || w.code === "reloaderRestarts")
  )
}

const same = (a: WorkloadRef, b: WorkloadRef) => a.kind === b.kind && a.name === b.name

function rolling(t: Translate, workload: WorkloadRef, podsOf: PodsOf, restart: boolean): string {
  const pods = podsOf(workload)
  const ready = pods?.ready ?? 0
  if (ready >= 2) return t("confirm.rolling", workload.name, ready)
  if (ready === 1 || pods?.desired === 1)
    return restart
      ? t("confirm.restartSingle", workload.name)
      : t("confirm.rollingSingle", workload.name)
  return restart ? t("confirm.restartNone", workload.name) : t("confirm.rollingNone", workload.name)
}

/** The plan's field changes of one object. */
const fieldsOf = (plan: Pick<Plan, "objects"> | undefined, kind: string, name: string) =>
  plan?.objects.find((o) => o.target.kind === kind && o.target.name === name)?.changes ?? []

/** Whether the plan's fields under an entry (`env[name=A]`, `data.A`) are all new. */
function isNew(
  fields: readonly Schemas["FieldChange"][],
  under: (path: string) => boolean,
): boolean {
  const own = fields.filter((f) => under(f.path))
  return own.length > 0 && own.every((f) => f.before === undefined || f.before === null)
}

const below = (path: string, entry: string) => path === entry || path.startsWith(`${entry}.`)

/** Removed, added and changed names: the riskiest first. */
function sorted<E extends { op: string }>(
  edits: readonly E[],
  nameOf: (edit: E) => string,
  added: (name: string) => boolean,
): [string[], string[], string[]] {
  const removed = edits.filter((e) => e.op === "remove").map(nameOf)
  const set = edits.filter((e) => e.op !== "remove").map(nameOf)
  return [removed, set.filter(added), set.filter((n) => !added(n))]
}

const databases = new Set([
  "clusters.postgresql.cnpg.io",
  "perconaservermongodbs.psmdb.percona.com",
])

function sentenceOf(
  t: Translate,
  change: Change,
  podsOf: PodsOf,
  plan?: Pick<Plan, "objects">,
): string {
  const list = (items: string[]) => (items.length > 0 ? t("common.list", items) : "")
  switch (change.op) {
    case "restart":
      return rolling(t, change.workload, podsOf, true)
    case "env": {
      const fields = fieldsOf(plan, change.workload.kind, change.workload.name)
      const [removed, added, changed] = sorted(
        change.changes,
        (e) => e.name,
        (name) => isNew(fields, (path) => path.includes(`.env[name=${name}]`)),
      )
      return t("confirm.env", list(removed), list(added), list(changed), change.workload.name)
    }
    case "data": {
      const fields = fieldsOf(plan, change.kind, change.name)
      const [removed, added, changed] = sorted(
        change.changes,
        (e) => e.key,
        (key) =>
          isNew(fields, (path) => below(path, `data.${key}`) || below(path, `stringData.${key}`)),
      )
      return t("confirm.data", list(removed), list(added), list(changed), change.kind, change.name)
    }
    case "addDomain":
      return t("confirm.addDomain", change.hostname, change.workload.name)
    case "renameDomain":
      return t("confirm.renameDomain", change.from)
    case "removeDomain":
      return t("confirm.removeDomain", change.hostname)
    case "rollback":
      return t("confirm.rollback", change.revision)
    case "expandVolume":
      return t("confirm.noShrink")
    case "addVolume":
      return t("confirm.addVolume", change.volume.size, change.workload.name)
    case "removeVolume":
      return t("confirm.removeVolume", change.workload.name, change.mountPath)
    case "editMount": {
      const moved =
        change.newMountPath && change.newMountPath !== change.mountPath
          ? t("confirm.moveMount", change.workload.name, change.mountPath, change.newMountPath)
          : undefined
      const mode =
        change.readOnly === true
          ? t("confirm.readOnly", change.newMountPath ?? change.mountPath, change.workload.name)
          : change.readOnly === false
            ? t("confirm.writable", change.newMountPath ?? change.mountPath, change.workload.name)
            : undefined
      return [moved, mode].filter(Boolean).join(" ") || t("confirm.mount", change.workload.name)
    }
    case "probe":
      return probeSentence(t, change.probe, change.set ?? undefined)
    case "configReload":
      return change.enabled
        ? t("confirm.configReloadOn", change.workload.name)
        : t("confirm.configReloadOff", change.workload.name)
    case "securityProfile":
      return change.profile === "restricted"
        ? t("confirm.securityRestricted", change.workload.name)
        : t("confirm.securityBaseline", change.workload.name)
    case "podSecurity":
      return change.level === "restricted"
        ? t("confirm.podSecurityRestricted")
        : t("confirm.podSecurityBaseline")
    case "image":
      return t("confirm.image", change.workload.name, imageTag(change.image))
    case "rebuild":
      return t("confirm.rebuild", change.build)
    case "autoUpdate":
      return t(
        change.enabled ? "confirm.autoUpdateOn" : "confirm.autoUpdateOff",
        change.workload.name,
      )
    case "scale": {
      if (change.replicas === 0) return t("confirm.scaleZero", change.workload.name)
      const from = podsOf(change.workload)?.desired
      return from === undefined
        ? t("confirm.scaleTo", change.workload.name, change.replicas)
        : t("confirm.scale", change.workload.name, from, change.replicas)
    }
    case "resources":
      return resourcesSentence(
        t,
        change.requests ?? undefined,
        change.limits ?? undefined,
        fieldsOf(plan, change.workload.kind, change.workload.name),
      )
    case "edit":
      return t("confirm.yamlAsIs")
    case "create": {
      const object = (change.object ?? {}) as { kind?: unknown; metadata?: { name?: unknown } }
      const kind = typeof object.kind === "string" ? object.kind : ""
      const name = typeof object.metadata?.name === "string" ? object.metadata.name : ""
      return t("confirm.create", kind, name)
    }
    case "delete":
      return databases.has(`${change.resource.resource}.${change.resource.group}`)
        ? t("confirm.deleteDatabase", change.name)
        : t("confirm.delete", change.name)
    case "adopt":
      return t("confirm.adopt")
    case "createProject":
      return t("confirm.createProject")
    case "deployKey":
      return t("confirm.deployKey", change.name)
    case "unadopt":
      return t("confirm.unadopt")
    case "details":
      return t("confirm.details")
    case "debugContainer":
      return t("confirm.debugContainer", change.pod)
    case "clusterTools":
      return t("confirm.clusterTools")
    case "scheduleTask":
      return t("confirm.scheduleTask", change.name, change.from.name)
    case "runTask":
      return t("confirm.runTask", change.cronJob)
  }
}

/** Unset timings are Kubernetes' defaults
 * (https://kubernetes.io/docs/tasks/configure-pod-container/configure-liveness-readiness-startup-probes/#configure-probes). */
function probeSentence(
  t: Translate,
  probe: Schemas["ProbeKind"],
  set: Schemas["ProbeRequest"] | undefined,
): string {
  if (!set) return t(`confirm.probeRemoved.${probe}`)
  const port = String(set.port ?? "")
  const check =
    set.handler === "HTTPGet"
      ? t("confirm.check.http", port, set.path ?? "/")
      : set.handler === "TCPSocket"
        ? t("confirm.check.tcp", port)
        : set.handler === "GRPC"
          ? t("confirm.check.grpc", port)
          : t("confirm.check.exec", (set.command ?? []).join(" "))
  const period = set.periodSeconds ?? 10
  const failures = set.failureThreshold ?? 3
  return t(`confirm.probeSet.${probe}`, check, period, failures)
}

/** Each value set, with what it was ("100m → 250m"); one the change drops, as no longer set. */
function resourcesSentence(
  t: Translate,
  requests: Record<string, string> = {},
  limits: Record<string, string> = {},
  fields: readonly Schemas["FieldChange"][] = [],
): string {
  const labels: Record<string, [string, string]> = {
    cpu: [t("form.cpuRequest"), t("form.cpuLimit")],
    memory: [t("form.memoryRequest"), t("form.memoryLimit")],
  }
  const before = (bound: string, resource: string) =>
    fields.find((f) => f.path.endsWith(`.resources.${bound}.${resource}`))?.before ?? undefined
  const pairs: [string, string][] = []
  const shown = (bound: "requests" | "limits", values: Record<string, string>) => {
    const index = bound === "requests" ? 0 : 1
    for (const [resource, value] of Object.entries(values)) {
      const was = before(bound, resource)
      pairs.push([
        labels[resource]?.[index] ?? resource,
        was && was !== value ? `${was} → ${value}` : value,
      ])
    }
    for (const field of fields) {
      const match = new RegExp(`\\.resources\\.${bound}\\.([^.]+)$`).exec(field.path)
      const resource = match?.[1]
      if (!resource || resource in values || typeof field.after === "string") continue
      if (typeof field.before !== "string") continue
      pairs.push([labels[resource]?.[index] ?? resource, `${field.before} → ${t("form.notSet")}`])
    }
  }
  shown("requests", requests)
  shown("limits", limits)
  return t("confirm.resources", pairs)
}

/** `undefined` where the sentence of what happens already says it. */
function consequence(t: Translate, reason: Reason, service?: WorkloadRef): string | undefined {
  switch (reason) {
    case "fewPods":
    case "scaleToZero":
    case "deletesKeys":
    case "rollback":
      return undefined
    case "storage":
    case "yamlEdit":
    case "removesDomain":
      return undefined
    case "isolates":
      return t("confirm.isolates", service?.name ?? "")
    case "deletes":
      return undefined
  }
}

interface Fact {
  text: string
  tone?: "ok" | "warn"
}

export interface Description {
  consequence?: string
  /** For several changes of high risk: what they put at risk, together. */
  risk?: string
  lines: string[]
  /** The full names behind a line's "and N more", by line. */
  more: Record<string, string[]>
  action: string
  /** The workloads whose replicas the lines or the facts already speak of. */
  said: WorkloadRef[]
  /** What it costs beyond the lines: whether data stays, whether it can be undone. */
  cost?: string
  /** For many changes: the conclusion, kept in view above the groups. */
  facts?: Fact[]
  edits?: EditGroup[]
  outcomes?: OutcomeGroup[]
  /** The lines already say which fields go: a removal is what was asked. */
  removalsSaid?: boolean
}

export interface Asked {
  changes: readonly Change[]
  service?: WorkloadRef
}

export function describe(
  t: Translate,
  ask: Asked,
  plan: Plan,
  podsOf: PodsOf,
  reasons: readonly Reason[] = [],
  mountsOf: MountsOf = () => [],
): Description {
  if (writtenCreates(ask.changes)) return describeWritten(t, plan)
  const more: Record<string, string[]> = {}
  const named = (names: string[], sentence: (list: string) => string) => {
    const { text, rest } = bounded(t, names)
    const line = sentence(text)
    if (rest.length > 0) more[line] = names
    return line
  }
  const many = editCount(ask.changes) > FEW
  const outcomes =
    many && isBulk(ask.changes) ? outcomeGroups(t, ask.changes, plan, podsOf, mountsOf) : undefined
  const fields = many && !outcomes ? editGroups(t, plan) : []
  const edits = fields.length > 0 ? fields : undefined
  const lines: string[] = []
  const said: WorkloadRef[] = []
  const consequences = reasons
    .map((r) => consequence(t, r, ask.service))
    .filter((s): s is string => !!s)
  for (const change of ask.changes) {
    if (change.op === "restart") said.push(change.workload)
    // Grouped, the rows say what variables, compute and health checks change.
    if (outcomes) continue
    const part = partOf(change)
    if (edits && (part === "variables" || part === "compute" || part === "health-checks")) continue
    lines.push(sentenceOf(t, change, podsOf, plan))
  }
  if (edits && ask.service) said.push(ask.service)
  lines.push(...terminalLines(t, plan))
  if (ask.changes.some((c) => c.op === "rollback") && hasWarning(plan, "configNotRolledBack"))
    lines.push(t("confirm.notRolledBack"))
  for (const workload of replaced(plan))
    if (!said.some((w) => same(w, workload))) {
      lines.push(rolling(t, workload, podsOf, false))
      said.push(workload)
    }
  for (const change of ask.changes) if (change.op === "scale") said.push(change.workload)
  const waiting = plan.warnings.flatMap((w) => (w.code === "restartNeeded" ? w.workloads : []))
  said.push(...waiting)
  const data = ask.changes.find((c) => c.op === "data")
  const dataName = data?.op === "data" ? data.name : undefined
  if (waiting.length > 0)
    lines.push(
      named(
        waiting.map((w) => w.name),
        (names) =>
          dataName
            ? t("confirm.readersKeepOld", names, dataName, waiting.length)
            : t("confirm.keepsOld", names, waiting.length),
      ),
    )
  const reloader = plan.warnings.find((w) => w.code === "reloaderRestarts")
  const reloaded = reloader?.code === "reloaderRestarts" ? reloader.workloads : []
  said.push(...reloaded)
  if (reloader?.code === "reloaderRestarts")
    lines.push(
      named(
        reloaded.map((w) => w.name),
        (names) =>
          reloader.confirmed
            ? t("confirm.reloaderRestarts", names, reloaded.length)
            : t("confirm.reloaderMayRestart", names, reloaded.length),
      ),
    )
  // A deleted workload gets its own sentence, not an "also affects" one.
  const deleted = ask.changes.flatMap((c) => (c.op === "delete" ? [c.name] : []))
  const others = plan.impact.filter(
    (w) =>
      !(ask.service && same(w, ask.service)) &&
      !deleted.includes(w.name) &&
      !replaced(plan).some((r) => same(r, w)) &&
      !waiting.some((r) => same(r, w)) &&
      !reloaded.some((r) => same(r, w)),
  )
  const acting = new Set(["restart", "scale", "configReload"])
  if (others.length > 0 && !ask.changes.some((c) => acting.has(c.op)))
    lines.push(
      named(
        others.map((w) => w.name),
        (names) =>
          dataName
            ? t("confirm.alsoReads", names, dataName, others.length)
            : t("confirm.alsoAffects", names),
      ),
    )
  return {
    consequence: consequences.length > 0 ? consequences.join(" ") : undefined,
    risk: ask.changes.length > 1 ? riskOf(t, reasons, ask, podsOf, named) : undefined,
    lines,
    more,
    action: actionOf(t, ask),
    said,
    cost: costOf(t, ask, plan, podsOf, mountsOf),
    facts: outcomes
      ? outcomeFacts(t, outcomes, plan)
      : edits
        ? editFacts(t, edits, ask, plan, podsOf)
        : undefined,
    edits,
    outcomes,
    removalsSaid: ask.changes.every((c) => c.op === "configReload") || undefined,
  }
}

/** Whether the changes create objects a person wrote as YAML. */
function writtenCreates(changes: readonly Change[]): boolean {
  return changes.length > 0 && changes.every((c) => c.op === "create" && !!c.yaml)
}

/** Objects written as YAML, in the order the plan creates them. */
function describeWritten(t: Translate, plan: Plan): Description {
  const objects = plan.objects.map((o) => `${o.target.kind} ${o.target.name}`)
  const [one] = plan.objects
  const lines =
    one && plan.objects.length === 1
      ? [t("confirm.create", one.target.kind, one.target.name)]
      : [t("create.createsMany", objects.length, objects.join(t("common.sep")))]
  return {
    lines,
    more: {},
    action:
      plan.objects.length > 1 ? t("create.buttonMany", plan.objects.length) : t("action.create"),
    said: [],
    cost: plan.objects.length > 1 ? t("create.noTransaction") : undefined,
  }
}

type Named = (names: string[], sentence: (list: string) => string) => string

/** What several high-risk changes put at risk, one sentence per reason. */
function riskOf(
  t: Translate,
  reasons: readonly Reason[],
  ask: Asked,
  podsOf: PodsOf,
  named: Named,
): string | undefined {
  const sentences = reasons.map((reason) => {
    switch (reason) {
      case "fewPods": {
        const few = ask.changes.flatMap((c) =>
          c.op === "restart" && (podsOf(c.workload)?.ready ?? 0) < 2 ? [c.workload.name] : [],
        )
        return named(few, (names) => t("risk.fewPods", names, few.length))
      }
      case "scaleToZero": {
        const stopped = ask.changes.flatMap((c) =>
          c.op === "scale" && c.replicas === 0 ? [c.workload.name] : [],
        )
        return named(stopped, (names) => t("risk.scaleToZero", names, stopped.length))
      }
      case "deletesKeys":
        return t("risk.deletesKeys")
      case "rollback":
        return t("risk.rollback")
      case "storage":
        return t("risk.storage")
      case "yamlEdit":
        return t("confirm.yamlAsIs")
      case "removesDomain":
        return t("risk.removesDomain")
      case "isolates":
        return t("confirm.isolates", ask.service?.name ?? "")
      case "deletes":
        return t("risk.deletes")
    }
  })
  return sentences.length > 0 ? sentences.join(" ") : undefined
}

/** Whether data stays and whether it can be undone, where that is not obvious. */
function costOf(
  t: Translate,
  ask: Asked,
  plan: Plan,
  podsOf: PodsOf,
  mountsOf: MountsOf,
): string | undefined {
  const parts: string[] = []
  const touched = ask.changes.flatMap((c) =>
    c.op === "restart" || c.op === "scale" ? [c.workload] : [],
  )
  const keeping = touched.filter((w) => mountsOf(w).length > 0)
  const [one] = keeping
  if (one && (plan.preview?.dataLoss.length ?? 0) === 0)
    parts.push(
      touched.length === 1
        ? t("plan.cost.dataStaysOne", one.name, t("common.list", mountsOf(one)))
        : t("plan.cost.dataStays", keeping.length),
    )
  if (undoOf(ask.changes, podsOf)) parts.push(t("plan.cost.undo"))
  return parts.length > 0 ? parts.join(" ") : undefined
}

/** What happens when the new pods do not fit, and how to make room; undefined when they fit. */
export function shortfall(
  t: Translate,
  preview: Schemas["Preview"],
): { cost: string; fix: string } | undefined {
  const { fit } = preview
  if (fit.status !== "blocked") return undefined
  const blockers = fit.blockers ?? []
  const quota = blockers.some((b) => b.kind === "quota")
  const nodes = blockers.some((b) => b.kind === "node")
  const down = preview.rollouts
    .filter(
      (r) =>
        r.actions.includes("start") ||
        (r.actions.includes("roll") && r.downtime) ||
        (r.actions.includes("scale") && r.fromReplicas === 0),
    )
    .map((r) => r.workload.name)
  const names = bounded(t, down).text
  return {
    cost: quota
      ? t("plan.short.quota", names, down.length)
      : t("plan.short.nodes", names, down.length),
    fix: t("plan.short.fix", quota, nodes),
  }
}

function fitFact(t: Translate, plan: Plan): Fact[] {
  return plan.preview?.fit.status === "fits" ? [{ text: t("plan.fact.fits"), tone: "ok" }] : []
}

function outcomeFacts(t: Translate, groups: readonly OutcomeGroup[], plan: Plan): Fact[] {
  return [
    ...groups.map((g): Fact => ({
      text: t(`plan.outcome.${g.outcome}.count`, g.rows.length),
      tone: g.outcome === "up" || g.outcome === "starts" ? undefined : "warn",
    })),
    ...fitFact(t, plan),
  ]
}

function editFacts(
  t: Translate,
  groups: readonly EditGroup[],
  ask: Asked,
  plan: Plan,
  podsOf: PodsOf,
): Fact[] {
  const counts = countRows(groups)
  const facts: Fact[] = []
  if (counts.removed > 0) facts.push({ text: t("plan.fact.removed", counts.removed), tone: "warn" })
  if (counts.changed > 0) facts.push({ text: t("plan.fact.changed", counts.changed) })
  if (counts.added > 0) facts.push({ text: t("plan.fact.added", counts.added) })
  const scale = ask.changes.find((c) => c.op === "scale")
  if (scale?.op === "scale") {
    const from = podsOf(scale.workload)?.desired
    if (from !== undefined && from !== scale.replicas)
      facts.push({ text: t("plan.fact.replicas", from, scale.replicas) })
  }
  const service = ask.service
  const rollout = service
    ? plan.preview?.rollouts.find((r) => same(r.workload, service))
    : undefined
  if (rollout && rollout.actions.some((a) => a === "roll" || a === "scale"))
    facts.push(
      rollout.downtime
        ? { text: t("plan.fact.downBriefly"), tone: "warn" }
        : rollout.replicas === 0
          ? { text: t("plan.fact.stops"), tone: "warn" }
          : { text: t("plan.fact.staysUp") },
    )
  return [...facts, ...fitFact(t, plan)]
}

const minutes = (seconds: number) => Math.max(1, Math.ceil(seconds / 60))

/** What a debug container or tools pod plan means for the person, in sentences. */
export function terminalLines(t: Translate, plan: Pick<Plan, "warnings">): string[] {
  return plan.warnings.flatMap((w) => {
    switch (w.code) {
      case "debugContainerStays":
        return [t("confirm.debugStays")]
      case "debugContainerReused":
        return [t("confirm.debugReused")]
      case "toolsPodReused":
        return [t("confirm.toolsReused")]
      case "toolsPod": {
        const expires = w.credentialExpiresAt ? Date.parse(w.credentialExpiresAt) : NaN
        return [
          t("confirm.toolsActsAs", w.person, w.admin),
          Number.isNaN(expires)
            ? undefined
            : t("confirm.toolsCredential", minutes((expires - Date.now()) / 1000)),
          t("confirm.toolsStops", minutes(w.deadlineSeconds), minutes(w.graceSeconds)),
        ].filter((line): line is string => !!line)
      }
      default:
        return []
    }
  })
}

function hasWarning(plan: Plan, code: Schemas["PlanWarning"]["code"]): boolean {
  return plan.warnings.some((w) => w.code === code)
}

/** The undo toast's title: the one change's sentence, or what the many do, counted. */
export function undoTitle(t: Translate, undo: readonly Change[], asked: readonly Change[]): string {
  const [one] = undo
  if (one && undo.length === 1) return sentenceOf(t, one, () => undefined)
  if (undo.every((c) => c.op === "scale"))
    return asked.every((c) => c.op === "scale" && c.replicas === 0)
      ? t("undo.scaleUp", undo.length)
      : t("undo.scaleBack", undo.length)
  return t("undo.many", undo.length)
}

const storageOps = new Set<Change["op"]>(["addVolume", "expandVolume", "editMount", "removeVolume"])

export function actionOf(t: Translate, ask: Asked): string {
  // A deploy key opens into the create of its Secret: a form that sends one still creates.
  const ops = new Set(ask.changes.map((c) => (c.op === "deployKey" ? "create" : c.op)))
  const [change] = ask.changes
  if (!change) return t("plan.apply")
  if (writtenCreates(ask.changes))
    return ask.changes.length > 1 ? t("create.buttonMany", ask.changes.length) : t("action.create")
  if (ask.changes.length > 1 && [...ops].every((op) => storageOps.has(op)))
    return t("action.saveVolumes")
  if (ops.size > 1)
    // A service's draft, as its bar says: Save.
    return ask.changes.every((c) => partOf(c) !== undefined) ? t("action.save") : t("plan.apply")
  const count = ask.changes.length
  switch (change.op) {
    case "restart":
      return count > 1 ? t("action.restartMany", count) : t("action.restart")
    case "env":
      return t("action.saveVariables")
    case "data":
      return t("action.saveValues")
    case "configReload":
      return change.enabled ? t("action.turnOn") : t("action.turnOff")
    case "addDomain":
      return t("action.addDomain")
    case "renameDomain":
      return t("action.renameDomain")
    case "removeDomain":
      return t("action.removeDomain")
    case "rollback":
      return t("action.rollback", change.revision)
    case "expandVolume":
      return t("action.grow", change.size)
    case "addVolume":
      return t("action.addVolume")
    case "editMount":
      return t("action.saveMount")
    case "removeVolume":
      return t("action.removeVolume")
    case "probe":
      return t("action.saveHealthChecks")
    case "scale":
      if (ask.changes.every((c) => c.op === "scale" && c.replicas === 0))
        return count > 1 ? t("action.pauseMany", count) : t("action.pause")
      return count > 1 ? t("action.scaleMany", count) : t("action.scale", change.replicas)
    case "image":
      return t("action.deploy")
    case "rebuild":
      return t("action.rebuild")
    case "autoUpdate":
      return t(change.enabled ? "action.autoUpdateOn" : "action.autoUpdateOff")
    case "resources":
      return t("action.saveResources")
    case "edit":
      return t("action.saveYaml")
    case "create": {
      const object = (change.object ?? {}) as { kind?: unknown }
      if (object.kind !== "NetworkPolicy") return t("action.create")
      return isolates(change.object) ? t("action.isolate") : t("action.allow")
    }
    case "delete":
      return t("action.delete")
    case "adopt":
      return t("action.adopt")
    case "createProject":
    case "deployKey":
      return t("action.create")
    case "unadopt":
      return t("action.unadopt")
    case "details":
      return t("action.saveDetails")
    case "debugContainer":
      return t("action.openDebugTerminal")
    case "clusterTools":
      return t("action.openToolsTerminal")
    case "securityProfile":
      return change.profile === "restricted" ? t("action.makeStrict") : t("action.makeCompatible")
    case "podSecurity":
      return change.level === "restricted" ? t("action.makeStrict") : t("action.save")
    case "scheduleTask":
      return t("action.create")
    case "runTask":
      return t("tasks.runNow")
  }
}

export interface Failure {
  title: string
  description?: string
  review: boolean
}

/** Reads only the problem's typed members and keyed sentences, never its English wording. */
export function failureOf(t: Translate, locale: Locale, problem: Schemas["Problem"]): Failure {
  // Refused because Helm, Argo CD or Flux would undo it (a 409): reviewing again cannot help.
  const tool = problem.managedBy
  if (tool)
    return {
      title: t("toast.managed", tool),
      description: tool === "Helm" ? t("toast.managedHelm") : t("toast.managedGit", tool),
      review: false,
    }
  const text = problemText(t, locale, problem)
  return {
    title: text.headline,
    description: [text.why, text.fix].filter(Boolean).join(" "),
    review: problem.kind === "conflict" || problem.kind === "plan-expired",
  }
}

/** What becomes of a deleted claim's data. Its PersistentVolume and StorageClass are cluster-wide,
 * so a developer often cannot read the reclaim policy: then it says what usually happens, deleted
 * (the default of a dynamically provisioned volume), and when an admin can still recover it. */
export function claimDataSaid(
  t: Translate,
  warning: Extract<Schemas["PlanWarning"], { code: "claimDataMayBeLost" }>,
): { text: string; kept: boolean } {
  switch (warning.reclaimPolicy) {
    case "Retain":
      return { text: t("warn.claimDataRetained", warning.claim, warning.volume ?? ""), kept: true }
    case "Delete":
      return { text: t("warn.claimDataDeleted", warning.claim, warning.volume ?? ""), kept: false }
    default:
      return { text: t("warn.claimDataUnlessRetained", warning.claim), kept: false }
  }
}
