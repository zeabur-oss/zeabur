import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import type { ReactNode } from "react"
import FileTextIcon from "~icons/ph/file-text"
import TerminalWindowIcon from "~icons/ph/terminal-window"
import WarningCircleFillIcon from "~icons/ph/warning-circle-fill"

import type { Schemas } from "@/api/client"
import { evidenceQuery, projectIncidentsQuery, queryClient } from "@/api/queries"
import { CopyText } from "@/components/copy-text"
import { OriginChips } from "@/components/origin"
import { Layers, Raw, type RawRow } from "@/components/problem"
import { buttonVariants } from "@zeabur/design-system/components/button"
import { useSentence, useT, writeSentence, type Sentence, type Translate } from "@/i18n"
import { serviceLink } from "@/lib/place"
import { writeBundle } from "@/model/bundle"
import { originOf } from "@/model/origin"
import { isTask, type Service } from "@/model/service"
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
  pod?: string
}

export function diagnosisAction(d: Diagnosis): DiagnosisAction {
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
    write(d.why),
    `${t("problem.whatToDo")}${t("common.colon")}${write(d.fix)}`,
    `${t(`origin.${origin}`)}${t("common.sep")}${t(`fixer.${fixer}`)}`,
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
  if (!diagnoses || diagnoses.length === 0) return null
  const button = buttonVariants({ variant: "outline", size: "sm" })
  const runs = !isTask(service.kind) && service.pods.some((p) => p.ready)
  return (
    <section
      aria-labelledby="diagnoses-title"
      className="rounded-lg border border-fail/30 bg-fail-soft/40"
    >
      <h2 id="diagnoses-title" className="sr-only">
        {t("diagnosis.title")}
      </h2>
      <ul className="flex flex-col">
        {diagnoses.map((d, i) => {
          const action = diagnosisAction(d)
          const raw = rawRows(t, d, service)
          const pod = action.pod ?? d.pods[0]
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
                <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1">
                  <p className="font-medium">{write(d.cause)}</p>
                  <OriginChips {...originOf(d)} />
                </div>
                {d.incident ? <IncidentCause project={project} id={d.incident} /> : null}
                <Layers
                  why={write(d.why)}
                  also={i === 0 ? also : undefined}
                  fix={write(d.fix)}
                  action={
                    <span className="flex flex-wrap items-center gap-1.5">
                      <Link
                        {...serviceLink(project, service, { tab: action.tab, pod: action.pod })}
                        className={button}
                      >
                        {t(action.label)}
                      </Link>
                      {action.tab === "logs" ? null : (
                        <Link
                          {...serviceLink(project, service, { tab: "logs", pod })}
                          className={button}
                        >
                          <FileTextIcon aria-hidden="true" data-icon="inline-start" />
                          {t("tab.logs")}
                        </Link>
                      )}
                      {runs ? (
                        <Link
                          {...serviceLink(project, service, { tab: "terminal", pod })}
                          className={button}
                        >
                          <TerminalWindowIcon aria-hidden="true" data-icon="inline-start" />
                          {t("tab.terminal")}
                        </Link>
                      ) : null}
                      <CopyText
                        value={async () => {
                          try {
                            const bundle = await queryClient.fetchQuery(
                              evidenceQuery(project, service.kind, service.name),
                            )
                            return writeBundle(bundle.markdown, (sentence) =>
                              writeSentence(t, sentence),
                            )
                          } catch {
                            return diagnosisText(t, write, d, service, project)
                          }
                        }}
                        label={t("diagnosis.copy")}
                      />
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
