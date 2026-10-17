// The plain layer of a write confirmation: one sentence of what will happen, the consequence of a
// high-risk change, who else it touches, and the name of the button that does it. Built from the
// change a page asked for, the plan Engawa answered with, and the pods the project reports.
// The dialog's title already names the action, so a sentence says its effect, with the values
// that decide it (a probe's port and interval, a container's requests), and never repeats the
// title. Pure: the same inputs give the same words, so each case is tested (describe.test.ts).
import type { Schemas } from "@/api/client"
import type { Locale, Translate } from "@/i18n"
import { problemText } from "@/model/problem"
import { isolates, type PodsOf, type Reason } from "@/model/risk"

type Change = Schemas["Change"]
type WorkloadRef = Schemas["WorkloadRef"]
type Plan = Pick<Schemas["PlanResponse"], "objects" | "impact" | "warnings">

const workloadKinds = new Set(["Deployment", "StatefulSet", "DaemonSet", "Job", "CronJob"])

/** The workloads whose pod template a plan changes: their pods are replaced. */
export function replaced(plan: Plan): WorkloadRef[] {
  return plan.objects
    .filter(
      (o) =>
        workloadKinds.has(o.target.kind) &&
        o.changes.some((c) => c.path === "spec.template" || c.path.startsWith("spec.template.")),
    )
    .map((o) => ({ kind: o.target.kind as WorkloadRef["kind"], name: o.target.name }))
}

const same = (a: WorkloadRef, b: WorkloadRef) => a.kind === b.kind && a.name === b.name

/** One sentence on how a workload's pods are replaced: one at a time with two or more ready,
 * with a short gap with one. */
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

/** One sentence of what a single change does. */
export function sentenceOf(t: Translate, change: Change, podsOf: PodsOf): string {
  const list = (items: string[]) => t("common.list", items)
  switch (change.op) {
    case "restart":
      return rolling(t, change.workload, podsOf, true)
    case "env": {
      const set = change.changes.filter((e) => e.op !== "remove").map((e) => e.name)
      const removed = change.changes.filter((e) => e.op === "remove").map((e) => e.name)
      const name = change.workload.name
      if (set.length > 0 && removed.length > 0)
        return t("confirm.envBoth", list(set), list(removed), name)
      return removed.length > 0
        ? t("confirm.envRemove", list(removed), name)
        : t("confirm.envSet", list(set), name)
    }
    case "data": {
      const set = change.changes.filter((e) => e.op !== "remove").map((e) => e.key)
      const removed = change.changes.filter((e) => e.op === "remove").map((e) => e.key)
      if (removed.length > 0)
        return set.length > 0
          ? t("confirm.dataBoth", list(set), list(removed), change.kind, change.name)
          : t("confirm.dataRemove", list(removed), change.kind, change.name)
      return t("confirm.dataSet", list(set), change.kind, change.name)
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
    case "scale": {
      if (change.replicas === 0) return t("confirm.scaleZero", change.workload.name)
      const from = podsOf(change.workload)?.desired
      return from === undefined
        ? t("confirm.scaleTo", change.workload.name, change.replicas)
        : t("confirm.scale", change.workload.name, from, change.replicas)
    }
    case "resources":
      return resourcesSentence(t, change.requests, change.limits)
    case "edit":
      return t("confirm.yamlAsIs")
    case "create": {
      const object = change.object as { kind?: unknown; metadata?: { name?: unknown } }
      const kind = typeof object.kind === "string" ? object.kind : ""
      const name = typeof object.metadata?.name === "string" ? object.metadata.name : ""
      return t("confirm.create", kind, name)
    }
    case "delete":
      return t("confirm.delete", change.name)
    case "adopt":
      return t("confirm.adopt")
    case "unadopt":
      return t("confirm.unadopt")
    case "details":
      return t("confirm.details")
  }
}

/** What a probe does once set, or what goes without it once removed. Unset timings are
 * Kubernetes' defaults: every 10 seconds, 3 failures in a row
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

/** The requests and limits a resources change sets, as the compute form names them. */
function resourcesSentence(
  t: Translate,
  requests: Record<string, string> = {},
  limits: Record<string, string> = {},
): string {
  const labels: Record<string, [string, string]> = {
    cpu: [t("form.cpuRequest"), t("form.cpuLimit")],
    memory: [t("form.memoryRequest"), t("form.memoryLimit")],
  }
  const pairs: [string, string][] = []
  for (const [resource, value] of Object.entries(requests))
    pairs.push([labels[resource]?.[0] ?? resource, value])
  for (const [resource, value] of Object.entries(limits))
    pairs.push([labels[resource]?.[1] ?? resource, value])
  return t("confirm.resources", pairs)
}

/** What a high-risk reason costs, said before anything else; `undefined` where the sentence of
 * what happens already says it (a one-pod restart, scaling to zero, a rollback, a deleted key,
 * a grown volume, a YAML edit, a removed domain, a deleted object). */
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

export interface Description {
  /** For a high-risk change: what it costs, said first. */
  consequence?: string
  /** What will happen, in order. */
  lines: string[]
  /** The primary button. */
  action: string
}

export interface Asked {
  changes: readonly Change[]
  restartAfter?: boolean
  /** The service the change was asked from. */
  service?: WorkloadRef
}

/** The plain sentences and the button for a planned change. */
export function describe(
  t: Translate,
  ask: Asked,
  plan: Plan,
  podsOf: PodsOf,
  reasons: readonly Reason[] = [],
): Description {
  const list = (items: string[]) => t("common.list", items)
  const lines: string[] = []
  const said: WorkloadRef[] = []
  // Of a high-risk plan, the sentence that carries the consequence comes first.
  const consequences = reasons
    .map((r) => consequence(t, r, ask.service))
    .filter((s): s is string => !!s)
  for (const change of ask.changes) {
    lines.push(sentenceOf(t, change, podsOf))
    if (change.op === "restart") said.push(change.workload)
  }
  if (ask.changes.some((c) => c.op === "rollback") && hasWarning(plan, "configNotRolledBack"))
    lines.push(t("confirm.notRolledBack"))
  for (const workload of replaced(plan))
    if (!said.some((w) => same(w, workload))) lines.push(rolling(t, workload, podsOf, false))
  const waiting = plan.warnings.flatMap((w) => (w.code === "restartNeeded" ? w.workloads : []))
  const data = ask.changes.find((c) => c.op === "data")
  const dataName = data?.op === "data" ? data.name : undefined
  if (waiting.length > 0) {
    const names = list(waiting.map((w) => w.name))
    lines.push(
      dataName
        ? t("confirm.readersKeepOld", names, dataName, waiting.length)
        : t("confirm.keepsOld", names, waiting.length),
    )
  }
  // A deleted workload is the subject of its own sentence, not one the change also affects.
  const deleted = ask.changes.flatMap((c) => (c.op === "delete" ? [c.name] : []))
  const others = plan.impact.filter(
    (w) =>
      !(ask.service && same(w, ask.service)) &&
      !deleted.includes(w.name) &&
      !replaced(plan).some((r) => same(r, w)) &&
      !waiting.some((r) => same(r, w)),
  )
  if (others.length > 0 && !ask.changes.some((c) => c.op === "restart" || c.op === "scale")) {
    const names = list(others.map((w) => w.name))
    lines.push(
      dataName
        ? t("confirm.alsoReads", names, dataName, others.length)
        : t("confirm.alsoAffects", names),
    )
  }
  return {
    consequence: consequences.length > 0 ? consequences.join(" ") : undefined,
    lines,
    action: actionOf(t, ask, plan),
  }
}

function hasWarning(plan: Plan, code: Schemas["PlanWarning"]["code"]): boolean {
  return plan.warnings.some((w) => w.code === code)
}

/** The primary button: the action named, never a bare "Apply" unless the changes are mixed. */
export function actionOf(t: Translate, ask: Asked, plan?: Plan): string {
  const ops = new Set(ask.changes.map((c) => c.op))
  const [change] = ask.changes
  if (!change) return t("plan.apply")
  if (ops.size > 1) {
    // The compute form: replicas and resources together.
    return [...ops].every((op) => op === "scale" || op === "resources")
      ? t("action.save")
      : t("plan.apply")
  }
  switch (change.op) {
    case "restart":
      return t("action.restart")
    case "env":
      return t("action.saveVariables")
    case "data":
      return ask.restartAfter && plan && hasWarning(plan, "restartAdded")
        ? t("action.saveAndRestart")
        : t("action.saveValues")
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
    case "probe":
      return t("action.saveHealthChecks")
    case "scale":
      return t("action.scale", change.replicas)
    case "resources":
      return t("action.saveResources")
    case "edit":
      return t("action.saveYaml")
    case "create": {
      const object = change.object as { kind?: unknown }
      if (object.kind !== "NetworkPolicy") return t("action.create")
      return isolates(change.object) ? t("action.isolate") : t("action.allow")
    }
    case "delete":
      return t("action.delete")
    case "adopt":
      return t("action.adopt")
    case "unadopt":
      return t("action.unadopt")
    case "details":
      return t("action.saveDetails")
  }
}

/** What a toast says about a failed apply: the problem's headline, why and what to do (its
 * evidence and raw answer stay in the plan dialog), and whether reviewing the change again can help
 * (the object moved on, or the plan's time ran out). */
export interface Failure {
  title: string
  description?: string
  review: boolean
}

/** The failure of an apply, from its problem (RFC 9457). Only the problem's typed members and keyed
 * sentences are read (`managedBy`, `kind`, `summary`, `why`, `fixKey`), never the wording of its
 * English messages. */
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
