import { Link } from "@tanstack/react-router"
import type { ReactNode } from "react"
import FileTextIcon from "~icons/ph/file-text"

import { diagnosisAction, DiagnosisLine } from "@/components/diagnoses"
import { FixerLine } from "@/components/origin"
import { ServiceIcon } from "@/components/service-icon"
import { ServiceStatus } from "@/components/status"
import { buttonVariants } from "@zeabur/design-system/components/button"
import { useT } from "@/i18n"
import { serviceLink } from "@/lib/place"
import { originOf } from "@/model/origin"
import { restartsOf, statusOf, worstPod, type Service } from "@/model/service"

/** One service in trouble: what is wrong, where it comes from, and the one step that fixes it. */
export function ProblemRow({
  project,
  service,
  showProject = false,
  children,
}: {
  project: string
  service: Service
  showProject?: boolean
  /** Under the cause: what else the problem reaches. */
  children?: ReactNode
}) {
  const t = useT()
  const status = statusOf(service)
  const pod = worstPod(service)
  const first = service.diagnoses?.[0]
  const action = first ? diagnosisAction(first) : undefined
  const restarts = restartsOf(service)
  const facts = [
    status.desired !== undefined ? t("attention.ready", status.ready ?? 0, status.desired) : "",
    restarts > 0 ? t("pod.restarts", restarts) : "",
  ].filter(Boolean)
  const button = buttonVariants({ variant: "outline", size: "sm" })
  return (
    <li className="flex flex-wrap items-center gap-x-4 gap-y-2.5 border-t border-rule px-4 py-3 first:border-t-0">
      <ServiceIcon service={service} />
      <div className="flex min-w-60 flex-1 flex-col gap-1">
        <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1">
          <span className="flex min-w-0 items-baseline gap-1.5">
            {showProject ? (
              <>
                <Link
                  to="/projects/$project"
                  params={{ project }}
                  className="type-meta text-muted-foreground hover:text-foreground hover:underline"
                  translate="no"
                >
                  {project}
                </Link>
                <span aria-hidden="true" className="type-meta text-muted-foreground">
                  /
                </span>
              </>
            ) : null}
            <Link
              {...serviceLink(project, service)}
              className="truncate font-semibold hover:underline"
              translate="no"
            >
              {service.name}
            </Link>
          </span>
          <ServiceStatus status={status} size="sm" />
          {facts.length > 0 ? (
            <span className="type-meta text-muted-foreground">{facts.join(t("common.sep"))}</span>
          ) : null}
        </div>
        {first ? (
          <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1">
            <DiagnosisLine diagnoses={service.diagnoses} />
            <FixerLine fixer={originOf(first).fixer} />
          </div>
        ) : null}
        {children}
      </div>
      <div className="flex shrink-0 flex-wrap items-center gap-1.5">
        {action && action.tab !== "logs" ? (
          <Link
            {...serviceLink(project, service, { tab: action.tab, pod: action.pod })}
            className={button}
          >
            {t(action.label)}
          </Link>
        ) : null}
        <Link
          {...serviceLink(project, service, { tab: "logs", pod: action?.pod ?? pod?.name })}
          className={button}
          aria-label={t("attention.logsOf", action?.pod ?? pod?.name ?? service.name)}
        >
          <FileTextIcon aria-hidden="true" data-icon="inline-start" />
          {t("tab.logs")}
        </Link>
        {action ? null : (
          <Link
            {...serviceLink(project, service, { tab: "events", pod: pod?.name })}
            className={button}
            aria-label={t("attention.eventsOf", service.name)}
          >
            {t("tab.events")}
          </Link>
        )}
      </div>
    </li>
  )
}
