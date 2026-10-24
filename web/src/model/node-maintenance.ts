// Maintenance moves replicas off a node in passes: each pass evicts what is still there, and a
// disruption budget can hold a replica back until another one elsewhere is ready. The browser
// repeats passes while the dialog is open; nothing repeats them once it is closed, and the node
// stays cordoned.
import type { Schemas } from "@/api/client"

type DrainProgress = Schemas["DrainProgress"]
type DrainPod = Schemas["DrainPod"]

/** The wait after a pass that only waits for replicas to stop, and the first wait for a budget. */
export const FIRST_WAIT_MS = 2_000
/** The longest wait for a budget. */
export const LONGEST_WAIT_MS = 30_000

/** Nothing another pass can change: every replica is gone, or what is left was refused or failed. */
export function settled(progress: DrainProgress): boolean {
  return progress.pods.every(
    (pod) => pod.outcome === "gone" || pod.outcome === "refused" || pod.outcome === "failed",
  )
}

/** Zeabur's own replica is leaving the node: the passes after it fail until Zeabur is up again
 * elsewhere, and that Zeabur does not know this browser's session. */
export function zeaburLeaving(progress: DrainProgress): boolean {
  return progress.pods.some((pod) => pod.zeabur && pod.outcome === "leaving")
}

/** How long to wait before the next pass: while a budget holds a replica, twice the last wait (at
 * least what kube-apiserver asked for, at most 30 s); else 2 s. */
export function nextWait(progress: DrainProgress, previous: number | undefined): number {
  const waiting = progress.pods.filter((pod) => pod.outcome === "waiting")
  if (waiting.length === 0) return FIRST_WAIT_MS
  const asked = Math.max(0, ...waiting.map((pod) => (pod.retryAfterSeconds ?? 0) * 1000))
  const doubled = previous === undefined ? FIRST_WAIT_MS : previous * 2
  return Math.min(LONGEST_WAIT_MS, Math.max(asked, doubled))
}

export interface DrainLoop {
  /** The answer of apply, or of the pass before. */
  first: DrainProgress
  pass: () => Promise<DrainProgress>
  sleep: (ms: number, signal: AbortSignal) => Promise<void>
  signal: AbortSignal
  onProgress: (progress: DrainProgress) => void
}

/** Repeats passes until nothing another pass can change, until `signal` aborts, or until a pass
 * fails while Zeabur itself is leaving. */
export async function drain({
  first,
  pass,
  sleep,
  signal,
  onProgress,
}: DrainLoop): Promise<DrainProgress> {
  let progress = first
  let wait: number | undefined
  while (!settled(progress) && !signal.aborted) {
    const holding = progress.pods.some((pod) => pod.outcome === "waiting")
    wait = nextWait(progress, holding ? wait : undefined)
    await sleep(wait, signal)
    if (signal.aborted) break
    try {
      progress = await pass()
    } catch (error) {
      if (zeaburLeaving(progress)) break
      throw error
    }
    onProgress(progress)
  }
  return progress
}

export const sleep = (ms: number, signal: AbortSignal) =>
  new Promise<void>((resolve) => {
    const timer = setTimeout(resolve, ms)
    signal.addEventListener(
      "abort",
      () => {
        clearTimeout(timer)
        resolve()
      },
      { once: true },
    )
  })

export interface PodGroup {
  key: string
  /** The project's name, or the namespace when it is not a project. */
  label: string
  project?: string
  /** Each workload once: how many of its replicas are here, whether one keeps scratch space, and
   * the disruption budget that holds them now. */
  workloads: { name: string; count: number; scratch: boolean; budget?: string }[]
  pods: DrainPod[]
}

/** Pods by project, then by workload, in the plan's order. */
export function byProject(pods: readonly DrainPod[]): PodGroup[] {
  const groups = new Map<string, PodGroup>()
  for (const pod of pods) {
    const group = groups.get(pod.namespace) ?? {
      key: pod.namespace,
      label: pod.projectName ?? pod.namespace,
      project: pod.project ?? undefined,
      workloads: [],
      pods: [],
    }
    groups.set(pod.namespace, group)
    group.pods.push(pod)
    const name = pod.workload?.name ?? pod.name
    const budget = pod.budgetBlocks ? (pod.budget ?? undefined) : undefined
    const workload = group.workloads.find((w) => w.name === name)
    if (workload) {
      workload.count += 1
      workload.scratch ||= pod.scratch.length > 0
      workload.budget ??= budget
    } else group.workloads.push({ name, count: 1, scratch: pod.scratch.length > 0, budget })
  }
  return [...groups.values()].toSorted(
    (a, b) => Number(Boolean(b.project)) - Number(Boolean(a.project)) || a.key.localeCompare(b.key),
  )
}

/** The pods of a plan by what happens to them. */
export function fates(pods: readonly DrainPod[]) {
  const of = (fate: Schemas["PodFate"]) => pods.filter((pod) => pod.fate === fate)
  return { moves: of("moves"), stuck: of("stuck"), deleted: of("deleted"), stays: of("stays") }
}

/** What to run on a removed node's machine: RKE2's own uninstaller, then the installer's files. */
export const UNINSTALL_COMMANDS =
  "/usr/local/bin/rke2-uninstall.sh\nzeabur-installer uninstall --purge"

/** What kubectl does for the same plan. */
export function kubectlCommand(
  action: Schemas["NodeAction"],
  node: string,
  force: boolean,
): string {
  if (action === "cordon") return `kubectl cordon ${node}`
  if (action === "uncordon") return `kubectl uncordon ${node}`
  if (action === "remove") return `kubectl delete node ${node}`
  return `kubectl drain ${node} --ignore-daemonsets --delete-emptydir-data${force ? " --force" : ""}`
}
