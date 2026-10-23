import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import type { ReactNode } from "react"
import FileTextIcon from "~icons/ph/file-text"
import TerminalWindowIcon from "~icons/ph/terminal-window"
import WarningCircleFillIcon from "~icons/ph/warning-circle-fill"

import type { Schemas } from "@/api/client"
import { useLogsReadable } from "@/api/logs-access"
import { evidenceQuery, projectIncidentsQuery, queryClient } from "@/api/queries"
import { CopyText } from "@/components/copy-text"
import { fixLead, originSentence } from "@/components/origin"
import { Layers, Raw, type RawRow } from "@/components/problem"
import { Button, buttonVariants } from "@zeabur/design-system/components/button"
import { useSentence, useT, writeSentence, type Sentence, type Translate } from "@/i18n"
import { openShell } from "@/lib/dock"
import { serviceLink } from "@/lib/place"
import { writeBundle } from "@/model/bundle"
import { originOf } from "@/model/origin"
import { isTask, statusOf, type Service } from "@/model/service"
import { troubleTone } from "@/model/severity"
import { cn } from "@/lib/utils"
import type { ServicePlace } from "@/routes/search"

export type Diagnosis = Schemas["Diagnosis"]

/** The cluster problem behind a symptom, which fixing the service itself does not cure. */
function IncidentCause({ project, id }: { project: string; id: string }) {
  const t = useT()
  const write = useSentence()
  const incidents = useQuery(projectIncidentsQuery(project))
  const incident = incidents.data?.items.find((i) => i.id === id)
  if (!incident) return null
  return (
    <p className="type-meta text-muted-foreground">
      {t("incident.causedBy")}
      {t("common.colon")}
      <Link to="/" className="text-foreground hover:underline">
        {write(incident.cause)}
      </Link>
    </p>
  )
}

export interface DiagnosisAction {
  tab: ServicePlace
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
    | "diagnosis.action.yaml"
  pod?: string
}

/** The step that fixes `d`; without `logs`, the events instead of the logs. */
export function diagnosisAction(d: Diagnosis, logs = true): DiagnosisAction {
  const action = actionOf(d)
  return action.tab === "logs" && !logs
    ? { tab: "events", label: "diagnosis.action.events", pod: action.pod }
    : action
}

function actionOf(d: Diagnosis): DiagnosisAction {
  const pod = d.pods[0]
  const cause = d.cause.key
  switch (d.kind) {
    case "crashLoop":
    case "runFailed":
      return { tab: "logs", label: "diagnosis.action.logs", pod }
    case "outOfMemory":
      return { tab: "compute", label: "diagnosis.action.memory" }
    case "probeFailure":
      return { tab: "health-checks", label: "diagnosis.action.healthChecks" }
    case "unreachable":
      return cause.endsWith(".noReadyEndpoints")
        ? { tab: "events", label: "diagnosis.action.events" }
        : { tab: "networking", label: "diagnosis.action.networking" }
    case "quotaBlocked":
      return { tab: "compute", label: "diagnosis.action.replicas" }
    case "unschedulable":
      return cause.endsWith(".cpu") || cause.endsWith(".memory")
        ? { tab: "compute", label: "diagnosis.action.reservation" }
        : { tab: "events", label: "diagnosis.action.events", pod }
    case "configMissing":
      return cause.endsWith(".volume")
        ? { tab: "storage", label: "diagnosis.action.storage" }
        : { tab: "variables", label: "diagnosis.action.variables" }
    case "unexplained":
      if (d.fix.key === "fix.multiAttach") return { tab: "yaml", label: "diagnosis.action.yaml" }
      if (d.fix.key === "fix.outOfMemory")
        return { tab: "compute", label: "diagnosis.action.memory" }
      return cause.endsWith(".notReady")
        ? { tab: "logs", label: "diagnosis.action.logs", pod }
        : { tab: "events", label: "diagnosis.action.events", pod }
    case "imagePull":
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

/** The diagnosis as plain text, for a ticket or a chat with whoever can fix it. */
function diagnosisText(
  t: Translate,
  write: (s: Sentence) => string,
  d: Diagnosis,
  service: Service,
  project: string,
): string {
  const { origin, fixer } = originOf(d)
  return [
    `${project}/${service.kind}/${service.name}`,
    write(d.cause),
    ...[originSentence(t, origin)].filter((s) => s !== undefined),
    write(d.why),
    `${fixLead(t, fixer) ?? `${t("problem.whatToDo")}${t("common.colon")}`}${write(d.fix)}`,
    ...rawRows(t, d, service).map(([label, value]) => `${label}: ${value}`),
    ...d.evidence.map((e) => `${t("diagnosis.evidence")}: ${e.kind} ${e.name}`),
    ...(d.lastSeen ? [`${t("diagnosis.lastSeen")}: ${d.lastSeen}`] : []),
  ].join("\n")
}

export function Diagnoses({
  diagnoses,
  service,
  project,
  also,
}: {
  diagnoses?: Diagnosis[]
  service: Service
  project: string
  /** What the first cause also takes down, inside its own entry. */
  also?: ReactNode
}) {
  const t = useT()
  const write = useSentence()
  const logs = useLogsReadable(project)
  if (!diagnoses || diagnoses.length === 0) return null
  const button = buttonVariants({ variant: "outline", size: "sm" })
  const runs = !isTask(service.kind) && service.pods.some((p) => p.ready)
  const warn = troubleTone(statusOf(service).health) === "warn"
  return (
    <section
      aria-labelledby="diagnoses-title"
      className={cn(
        "rounded-lg border",
        warn ? "border-warn/30 bg-warn-soft/40" : "border-fail/30 bg-fail-soft/40",
      )}
    >
      <h2 id="diagnoses-title" className="sr-only">
        {t("diagnosis.title")}
      </h2>
      <ul className="flex flex-col">
        {diagnoses.map((d, i) => {
          const action = diagnosisAction(d, logs)
          const raw = rawRows(t, d, service)
          const pod = action.pod ?? d.pods[0]
          const { origin, fixer } = originOf(d)
          const lead = fixLead(t, fixer)
          const where = lead ? undefined : originSentence(t, origin)
          const copy = (
            <CopyText
              primary={lead !== undefined}
              value={async () => {
                try {
                  const bundle = await queryClient.fetchQuery(
                    evidenceQuery(project, service.kind, service.name),
                  )
                  return writeBundle(bundle.markdown, (sentence) => writeSentence(t, sentence))
                } catch {
                  return diagnosisText(t, write, d, service, project)
                }
              }}
              label={t("diagnosis.copy")}
            />
          )
          return (
            <li
              key={`${d.kind}/${d.container ?? ""}/${d.cause.key}`}
              className={cn(
                "flex gap-3 border-t px-4 py-3 text-sm first:border-t-0",
                warn ? "border-warn/20" : "border-fail/20",
              )}
            >
              <WarningCircleFillIcon
                className={cn("mt-0.5 size-4 shrink-0", warn ? "text-warn" : "text-fail")}
                aria-hidden="true"
              />
              <div className="flex min-w-0 flex-1 flex-col gap-1.5">
                <p className="font-medium">{write(d.cause)}</p>
                {where ? <p className="text-secondary">{where}</p> : null}
                {d.incident ? <IncidentCause project={project} id={d.incident} /> : null}
                <Layers
                  why={write(d.why)}
                  also={i === 0 ? also : undefined}
                  fix={write(d.fix)}
                  fixLead={lead}
                  action={
                    <span className="flex flex-wrap items-center gap-1.5">
                      {lead ? copy : null}
                      <Link
                        {...serviceLink(project, service, { tab: action.tab, pod: action.pod })}
                        className={button}
                      >
                        {t(action.label)}
                      </Link>
                      {action.tab === "logs" || !logs ? null : (
                        <Link
                          {...serviceLink(project, service, { tab: "logs", pod })}
                          className={button}
                        >
                          <FileTextIcon aria-hidden="true" data-icon="inline-start" />
                          {t("tab.logs")}
                        </Link>
                      )}
                      {runs ? (
                        <Button
                          variant="outline"
                          size="sm"
                          onClick={() => openShell(project, service, pod)}
                        >
                          <TerminalWindowIcon aria-hidden="true" data-icon="inline-start" />
                          {t("tab.terminal")}
                        </Button>
                      ) : null}
                      {lead ? null : copy}
                    </span>
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
