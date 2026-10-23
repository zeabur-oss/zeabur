// Carrying out a confirmed plan, after its dialog has closed: one toast follows it from "Applying…"
// to the outcome. Success names what changed and offers View (where to see it) or Undo (where the
// change can be taken back, model/risk.ts). When the project moved on since the review and nothing
// was written, the change is planned again and shown with what is new (replan.ts). Other failures
// give the reason in plain words and, when the plan ran out, "Review again"; when the
// cluster refused it and the server offers admin rights, "Retry with admin rights", which applies
// the same plan again after the passkey confirmation (auth/elevation.ts); when Helm would undo it,
// the kubectl terminal with `helm list` typed in.
import { toast } from "sonner"

import { api, toProblem, unwrap, type Schemas } from "@/api/client"
import { queryClient, toolsQuery } from "@/api/queries"
import { retryAsAdmin } from "@/auth/elevation"
import { failureOf } from "@/components/plan/describe"
import { markCreated } from "@/lib/created"
import { openKubectl } from "@/lib/dock"
import type { PlanAsk } from "@/components/plan/planner"
import { replansOn, type Reviewed } from "@/components/plan/replan"
import type { Locale, Translate } from "@/i18n"
import { namespaceOps } from "@/model/project"

/** Toasts with an action stay long enough to reach it. */
const WITH_ACTION_MS = 10_000

async function apply(project: string, token: string) {
  return unwrap(
    await api.POST("/api/v1/projects/{project}/apply", {
      params: { path: { project } },
      body: { token },
    }),
  )
}

async function planAndApply(project: string, changes: Schemas["Change"][]) {
  const plan = unwrap(
    await api.POST("/api/v1/projects/{project}/plan", {
      params: { path: { project } },
      body: { changes },
    }),
  )
  return apply(project, plan.token)
}

export interface Confirmed {
  project: string
  token: string
  ask: PlanAsk
  t: Translate
  locale: Locale
  /** Where to see the change, a path and query. */
  view?: string
  /** The changes that take it back, from what the apply wrote, and their description for the
   * toast; planned and applied only when the person asks. */
  undo?: (
    applied: Schemas["Applied"][],
  ) => { changes: Schemas["Change"][]; title: string } | undefined
  navigate: (href: string) => void
  /** Opens the plan dialog for the same change again; with `reviewed`, after the apply was
   * refused because the project moved on. */
  reopen: (ask: PlanAsk, reviewed?: Reviewed) => void
  /** The plan the person confirmed. */
  reviewed?: Reviewed
  onApplied?: () => void
  /** The apply was refused after it created some of the objects. */
  onPartial?: (problem: Schemas["Problem"]) => void
}

/** Applies a confirmed plan and reports the outcome in a toast. */
export async function carryOut(confirmed: Confirmed): Promise<void> {
  const { project, token, ask, t, locale, view, navigate } = confirmed
  const id = toast.loading(t("plan.applying"), { description: ask.title })
  try {
    const { applied } = await apply(project, token)
    const undo = confirmed.undo?.(applied)
    markCreated(ask.changes)
    void queryClient.invalidateQueries({ queryKey: ["project", project] })
    // Adopting, letting go and the details change how the project list names it.
    if (ask.changes.some((c) => namespaceOps.has(c.op)))
      void queryClient.invalidateQueries({ queryKey: ["projects"] })
    confirmed.onApplied?.()
    if (ask.quiet) {
      toast.dismiss(id)
      return
    }
    toast.success(t("toast.applied"), {
      id,
      description: ask.title,
      duration: undo || view ? WITH_ACTION_MS : undefined,
      action: undo
        ? {
            label: t("toast.undo"),
            onClick: () => void takeBack(project, undo.changes, t, locale, undo.title),
          }
        : view
          ? { label: t("toast.view"), onClick: () => navigate(view) }
          : undefined,
    })
  } catch (error) {
    const problem = toProblem(error)
    if (problem.applied?.length && confirmed.onPartial) {
      toast.dismiss(id)
      void queryClient.invalidateQueries({ queryKey: ["project", project] })
      confirmed.onPartial(problem)
      return
    }
    if (replansOn(problem)) {
      toast.dismiss(id)
      void queryClient.invalidateQueries({ queryKey: ["project", project] })
      confirmed.reopen(ask, confirmed.reviewed ?? { objects: [] })
      return
    }
    void fail(
      project,
      id,
      t,
      locale,
      error,
      () => confirmed.reopen(ask),
      () => void carryOut(confirmed),
    )
  }
}

async function takeBack(
  project: string,
  undo: Schemas["Change"][],
  t: Translate,
  locale: Locale,
  title: string,
) {
  const id = toast.loading(t("plan.applying"), { description: title })
  try {
    await planAndApply(project, undo)
    void queryClient.invalidateQueries({ queryKey: ["project", project] })
    toast.success(t("toast.undone"), { id, description: title })
  } catch (error) {
    void fail(project, id, t, locale, error)
  }
}

/** Reports a failed apply. `review` plans the change afresh; `again` applies the same plan once
 * more, which is what "Retry with admin rights" does after the passkey confirmation. A change Helm
 * would undo offers the kubectl terminal with `helm list` typed in, to find the release to
 * upgrade. */
async function fail(
  project: string,
  id: string | number,
  t: Translate,
  locale: Locale,
  error: unknown,
  review?: () => void,
  again?: () => void,
) {
  const problem = toProblem(error)
  const failure = failureOf(t, locale, problem)
  const action =
    problem.kind === "elevation-required" && again
      ? { label: t("elevate.retry"), onClick: () => void retryAsAdmin(again) }
      : failure.review && review
        ? { label: t("plan.replan"), onClick: () => review() }
        : problem.managedBy === "Helm" && (await toolsAvailable(project))
          ? {
              label: t("action.openInToolsTerminal"),
              onClick: () => openKubectl(project, { command: "helm list" }),
            }
          : undefined
  toast.error(failure.title, {
    id,
    description: failure.description,
    duration: WITH_ACTION_MS,
    action,
  })
}

const toolsAvailable = (project: string) =>
  queryClient.fetchQuery(toolsQuery(project)).then(
    (tools) => tools.available,
    () => false,
  )
