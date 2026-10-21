import type { ReactNode } from "react"

import type { Schemas } from "@/api/client"
import { RefusedKinds } from "@/components/refused"
import { StatusDot } from "@/components/status"
import { useT, type Translate } from "@/i18n"
import { useQuantityFormat } from "@/lib/quantity"
import type { Health } from "@/model/service"

type Preview = Schemas["Preview"]
type Rollout = Schemas["Rollout"]

function rolloutText(t: Translate, rollout: Rollout): string {
  const name = rollout.workload.name
  const parts = rollout.actions.map((action) => {
    switch (action) {
      case "roll":
        return rollout.strategy === "Recreate"
          ? t("preview.roll.recreate", name)
          : rollout.strategy === "OnDelete"
            ? t("preview.roll.onDelete", name)
            : t("preview.roll.rolling", name)
      case "scale":
        return t("preview.scale", name, rollout.fromReplicas ?? 0, rollout.replicas ?? 0)
      case "start":
        return t("preview.start", name)
      case "stop":
        return t("preview.stop", name)
      case "pendingRestart":
        return t("preview.pendingRestart", name)
    }
  })
  return parts.join(" ")
}

function Line({ health, children }: { health: Health; children: ReactNode }) {
  return (
    <li className="flex items-start gap-2 text-sm">
      <StatusDot health={health} className="mt-1.5" />
      <span className="min-w-0">{children}</span>
    </li>
  )
}

/** A variable removed by name, which the plan's sentence already names. */
const envEntry = /\.env\[name=[^\]]+\]/

/** Whether a rollout says more than the sentences: a strategy other than one replica at a time. */
const tellsMore = (r: Rollout) => r.strategy === "Recreate" || r.strategy === "OnDelete"

/** Why the plan's new pods do not fit, one sentence per blocker; empty when they do. */
export function useBlockers(fit: Schemas["Fit"]): string[] {
  const t = useT()
  const { cores, memory } = useQuantityFormat()
  const amount = (resource: string, value: number) =>
    resource.endsWith("cpu")
      ? t("preview.cores", cores(value))
      : resource.endsWith("memory")
        ? memory(value)
        : String(value)
  if (fit.status !== "blocked") return []
  return (fit.blockers ?? []).map((b) =>
    t(
      b.kind === "quota" ? "preview.blockedQuota" : "preview.blockedNodes",
      b.name ?? "",
      b.resource,
      amount(b.resource, b.need),
      amount(b.resource, b.available),
    ),
  )
}

/** What the plan does to running pods beyond what its sentences or rows already say. */
export function PlanPreview({
  preview,
  warnedClaims = [],
  said = [],
  rollouts: showRollouts = true,
  removals: showRemovals = true,
}: {
  preview: Preview
  /** Deleted claims a plan warning already describes, with the volume's reclaim policy. */
  warnedClaims?: string[]
  /** Workloads the sentences already speak of; their plain roll is not said again. */
  said?: readonly Schemas["WorkloadRef"][]
  /** False where a table already says what happens to each service. */
  rollouts?: boolean
  /** False where the rows already list the removed fields. */
  removals?: boolean
}) {
  const t = useT()
  const { cores, memory } = useQuantityFormat()
  const { fit } = preview
  const isSaid = (r: Rollout) =>
    said.some((w) => w.kind === r.workload.kind && w.name === r.workload.name)
  const rollouts = showRollouts
    ? preview.rollouts
        .filter((r) => tellsMore(r) || !isSaid(r))
        .toSorted((a, b) => Number(b.downtime) - Number(a.downtime))
    : []
  const removals = showRemovals
    ? preview.removals.filter((r) => r.leftOutOfEdit || !envEntry.test(r.path))
    : []
  const dataLoss = preview.dataLoss.filter(
    (loss) => !(loss.kind === "claimDeleted" && loss.claim && warnedClaims.includes(loss.claim)),
  )
  return (
    <div className="flex flex-col gap-3 empty:hidden">
      {rollouts.length > 0 ? (
        <section aria-label={t("preview.restarts")} className="flex flex-col gap-1.5">
          <h3 className="type-meta font-medium text-muted-foreground">{t("preview.restarts")}</h3>
          <ul className="flex flex-col gap-1.5">
            {rollouts.map((rollout) => (
              <Line
                key={`${rollout.workload.kind}/${rollout.workload.name}`}
                health={rollout.downtime ? "warn" : "idle"}
              >
                {rolloutText(t, rollout)}
                {rollout.downtime ? (
                  <span className="text-warn-strong"> {t("preview.downtime")}</span>
                ) : null}
              </Line>
            ))}
          </ul>
        </section>
      ) : null}
      {fit.status === "fits" ? (
        <ul>
          <Line health="ok">
            {t(
              "preview.fits",
              fit.pods,
              t("preview.cores", cores(fit.cpuMillicores)),
              memory(fit.memoryBytes),
            )}
          </Line>
        </ul>
      ) : fit.status === "unknown" ? (
        <ul>
          <Line health="idle">{t("preview.fitUnknown")}</Line>
        </ul>
      ) : null}
      {dataLoss.length > 0 ? (
        <ul className="flex flex-col gap-1.5" aria-label={t("preview.dataLoss")}>
          {dataLoss.map((loss, i) => {
            const paths = (loss.mountPaths ?? []).join(t("common.sep"))
            return (
              <Line key={`${loss.kind}/${loss.claim ?? loss.volume ?? i}`} health="fail">
                {loss.kind === "claimDeleted"
                  ? t("preview.loss.claimDeleted", loss.claim ?? "", paths)
                  : loss.kind === "emptyDir"
                    ? t(
                        "preview.loss.emptyDir",
                        loss.workload?.name ?? "",
                        loss.volume ?? "",
                        paths,
                      )
                    : t("preview.loss.statefulSet", loss.workload?.name ?? "", loss.claim ?? "")}
              </Line>
            )
          })}
        </ul>
      ) : null}
      {removals.length > 0 ? (
        <ul className="flex flex-col gap-1.5" aria-label={t("preview.removals")}>
          {removals.map((r) => (
            <Line key={`${r.target.kind}/${r.target.name}/${r.path}`} health="warn">
              {t("preview.removal", r.path, r.target.name)}{" "}
              <span className="font-mono text-xs text-muted-foreground" translate="no">
                {r.before}
              </span>
              {r.leftOutOfEdit ? (
                <span className="text-muted-foreground"> {t("preview.leftOut")}</span>
              ) : null}
            </Line>
          ))}
        </ul>
      ) : null}
      <RefusedKinds unreadable={preview.unreadable} line />
    </div>
  )
}
