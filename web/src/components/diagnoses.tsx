import WarningCircleFillIcon from "~icons/ph/warning-circle-fill"

import type { Schemas } from "@/api/client"
import { Layers, Raw, type RawRow } from "@/components/problem"
import { Button } from "@zeabur/design-system/components/button"
import { useSentence, useT, type Translate } from "@/i18n"
import type { Service } from "@/model/service"
import type { ServiceTab } from "@/routes/search"
import { healthChecksSection } from "@/lib/stores"

export type Diagnosis = Schemas["Diagnosis"]

export interface DiagnosisAction {
  tab: ServiceTab
  label:
    | "diagnosis.action.logs"
    | "diagnosis.action.memory"
    | "diagnosis.action.healthChecks"
    | "diagnosis.action.networking"
    | "diagnosis.action.events"
    | "diagnosis.action.replicas"
    | "diagnosis.action.reservation"
    | "diagnosis.action.variables"
    | "diagnosis.action.storage"
  pod?: string
  /** A folded section of that tab to open. */
  opens?: "healthChecks"
}

export function diagnosisAction(d: Diagnosis): DiagnosisAction {
  const pod = d.pods[0]
  const cause = d.cause.key
  switch (d.kind) {
    case "crashLoop":
    case "runFailed":
      return { tab: "logs", label: "diagnosis.action.logs", pod }
    case "outOfMemory":
      return { tab: "settings", label: "diagnosis.action.memory" }
    case "probeFailure":
      return { tab: "settings", label: "diagnosis.action.healthChecks", opens: "healthChecks" }
    case "unreachable":
      return cause.endsWith(".noReadyEndpoints")
        ? { tab: "events", label: "diagnosis.action.events" }
        : { tab: "networking", label: "diagnosis.action.networking" }
    case "quotaBlocked":
      return { tab: "settings", label: "diagnosis.action.replicas" }
    case "unschedulable":
      return cause.endsWith(".cpu") || cause.endsWith(".memory")
        ? { tab: "settings", label: "diagnosis.action.reservation" }
        : { tab: "events", label: "diagnosis.action.events", pod }
    case "configMissing":
      return cause.endsWith(".volume")
        ? { tab: "storage", label: "diagnosis.action.storage" }
        : { tab: "variables", label: "diagnosis.action.variables" }
    case "imagePull":
    case "unexplained":
      return { tab: "events", label: "diagnosis.action.events", pod }
  }
}

export function DiagnosisLine({ diagnoses }: { diagnoses?: Diagnosis[] }) {
  const write = useSentence()
  const first = diagnoses?.[0]
  if (!first) return null
  return <span className="type-meta text-foreground">{write(first.cause)}</span>
}

function rawRows(t: Translate, d: Diagnosis, service?: Service): RawRow[] {
  const pod = service?.pods.find((p) => p.name === d.pods[0])
  const container = pod?.containerStatuses?.find((c) => c.name === d.container)
  const ended = container?.lastTermination ?? container?.terminated ?? undefined
  // Other service-wide kinds (traffic, quota, scheduling) show their evidence, not this reason.
  const reason =
    container?.waiting?.reason ??
    ended?.reason ??
    pod?.reason ??
    (d.kind === "unexplained" ? service?.status.reason : undefined)
  return [
    ...(pod ? [[t("diagnosis.replica"), pod.name] as const] : []),
    ...(reason ? [[t("problem.reason"), reason] as const] : []),
    ...(ended ? [[t("diagnosis.exitCode"), String(ended.exitCode)] as const] : []),
    ...(container && container.restarts > 0
      ? [[t("metric.restarts"), String(container.restarts)] as const]
      : []),
    ...(ended?.message ? [[t("problem.message"), ended.message] as const] : []),
  ]
}

export function Diagnoses({
  diagnoses,
  service,
  onAction,
}: {
  diagnoses?: Diagnosis[]
  service?: Service
  onAction?: (tab: ServiceTab, pod?: string) => void
}) {
  const t = useT()
  const write = useSentence()
  if (!diagnoses || diagnoses.length === 0) return null
  return (
    <section
      aria-labelledby="diagnoses-title"
      className="rounded-lg border border-fail/30 bg-fail-soft/40"
    >
      <h2 id="diagnoses-title" className="sr-only">
        {t("diagnosis.title")}
      </h2>
      <ul className="flex flex-col">
        {diagnoses.map((d) => {
          const action = diagnosisAction(d)
          const raw = rawRows(t, d, service)
          return (
            <li
              key={`${d.kind}/${d.container ?? ""}/${d.cause.key}`}
              className="flex gap-3 border-t border-fail/20 px-4 py-3 text-sm first:border-t-0"
            >
              <WarningCircleFillIcon
                className="mt-0.5 size-4 shrink-0 text-fail"
                aria-hidden="true"
              />
              <div className="flex min-w-0 flex-1 flex-col gap-1.5">
                <p className="font-medium">{write(d.cause)}</p>
                <Layers
                  why={write(d.why)}
                  fix={write(d.fix)}
                  action={
                    onAction ? (
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={() => {
                          if (action.opens === "healthChecks") healthChecksSection.set(true)
                          onAction(action.tab, action.pod)
                        }}
                      >
                        {t(action.label)}
                      </Button>
                    ) : null
                  }
                  evidence={d.evidence}
                  lastSeen={d.lastSeen}
                >
                  {raw.length > 0 ? <Raw rows={raw} /> : null}
                </Layers>
              </div>
            </li>
          )
        })}
      </ul>
    </section>
  )
}
