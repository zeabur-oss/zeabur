// The component installs and removals this tab drives. Kept here, outside the dialog, so an
// install goes on while the person moves between pages; the server runs nothing in the background.
import type { Schemas } from "@/api/client"
import { capabilitiesQuery, refresh } from "@/api/queries"
import { applyComponent } from "@/api/zeabur"
import { store } from "@/lib/stores"

/** How long the tab waits before it asks the server to go on. */
const POLL_MS = 3_000

export interface Install {
  id: string
  name: string
  action: Schemas.ComponentAction
  values: Schemas.ComponentRequestValues
  /** The plan of the last apply. */
  plan?: Schemas.ComponentPlan
  running: boolean
  error?: Error
}

export const installs = store<Install[]>([])

export const installOf = (list: Install[], id: string) => list.find((i) => i.id === id)

const update = (id: string, change: Partial<Install>) =>
  installs.set(installs.get().map((i) => (i.id === id ? { ...i, ...change } : i)))

const stopped = (plan: Schemas.ComponentPlan) =>
  plan.done || [...plan.steps, ...plan.checks].some((s) => s.state === "failed")

/** Applies the plan's next step, then asks again every few seconds until the plan is done or a
 * step or health check failed. One run per component; starting it again replaces the last. */
export function startInstall(
  job: Pick<Install, "id" | "name" | "action" | "values">,
  catalogVersion: string,
) {
  const current = installOf(installs.get(), job.id)
  if (current?.running) return
  installs.set([...installs.get().filter((i) => i.id !== job.id), { ...job, running: true }])
  void run(job, catalogVersion)
}

async function run(job: Pick<Install, "id" | "action" | "values">, version: string) {
  let catalogVersion = version
  for (;;) {
    try {
      const { plan } = await applyComponent(job.id, {
        action: job.action,
        catalogVersion,
        values: job.values,
      })
      if (!installOf(installs.get(), job.id)) return
      update(job.id, { plan, error: undefined })
      if (stopped(plan)) break
      catalogVersion = plan.catalogVersion
    } catch (error) {
      update(job.id, { error: error instanceof Error ? error : new Error(String(error)) })
      break
    }
    await new Promise((resolve) => setTimeout(resolve, POLL_MS))
  }
  update(job.id, { running: false })
  void refresh(capabilitiesQuery)
}

/** Forgets an install that no longer runs, once the person has seen how it ended. */
export function dismissInstall(id: string) {
  if (installOf(installs.get(), id)?.running) return
  installs.set(installs.get().filter((i) => i.id !== id))
}

/** Steps written so far and in all, for the indicator. */
export function progressOf(install: Install) {
  const steps = install.plan?.steps ?? []
  return { done: steps.filter((s) => s.state === "ready").length, total: steps.length }
}
