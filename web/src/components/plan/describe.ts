import type { Schemas } from "@/api/client"
import type { Locale, Translate } from "@/i18n"
import { problemText } from "@/model/problem"
import { isolates, type PodsOf, type Reason } from "@/model/risk"

type Change = Schemas["Change"]
type WorkloadRef = Schemas["WorkloadRef"]
type Plan = Pick<Schemas["PlanResponse"], "objects" | "impact" | "warnings">

const workloadKinds = new Set(["Deployment", "StatefulSet", "DaemonSet", "Job", "CronJob"])

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
    case "debugContainer":
      return t("confirm.debugContainer", change.pod)
    case "clusterTools":
      return t("confirm.clusterTools")
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

export interface Description {
  consequence?: string
  lines: string[]
  action: string
}

export interface Asked {
  changes: readonly Change[]
  restartAfter?: boolean
  service?: WorkloadRef
}

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
  const consequences = reasons
    .map((r) => consequence(t, r, ask.service))
    .filter((s): s is string => !!s)
  for (const change of ask.changes) {
    lines.push(sentenceOf(t, change, podsOf))
    if (change.op === "restart") said.push(change.workload)
  }
  lines.push(...terminalLines(t, plan))
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
  // A deleted workload gets its own sentence, not an "also affects" one.
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

const minutes = (seconds: number) => Math.max(1, Math.ceil(seconds / 60))

function terminalLines(t: Translate, plan: Plan): string[] {
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

const storageOps = new Set<Change["op"]>(["addVolume", "expandVolume", "editMount", "removeVolume"])

export function actionOf(t: Translate, ask: Asked, plan?: Plan): string {
  const ops = new Set(ask.changes.map((c) => c.op))
  const [change] = ask.changes
  if (!change) return t("plan.apply")
  if (ops.size > 1) {
    // The compute form: replicas and resources together.
    if ([...ops].every((op) => op === "scale" || op === "resources")) return t("action.save")
    return [...ops].every((op) => storageOps.has(op)) ? t("action.saveVolumes") : t("plan.apply")
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
    case "removeVolume":
      return t("action.removeVolume")
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
    case "debugContainer":
      return t("action.openDebugTerminal")
    case "clusterTools":
      return t("action.openToolsTerminal")
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
