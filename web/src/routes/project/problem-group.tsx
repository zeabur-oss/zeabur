import { Link } from "@tanstack/react-router"
import { Fragment, useState } from "react"
import FileTextIcon from "~icons/ph/file-text"

import { diagnosisAction } from "@/components/diagnoses"
import { FixerLine } from "@/components/origin"
import { ProblemRow } from "@/components/problem-row"
import { ServiceIcon } from "@/components/service-icon"
import { ServiceStatus } from "@/components/status"
import { buttonVariants } from "@zeabur/design-system/components/button"
import { useSentence, useT } from "@/i18n"
import { serviceLink } from "@/lib/place"
import { sharedCause, type ProblemGroup } from "@/model/attention"
import { originOf } from "@/model/origin"
import { serviceId, statusOf, worstPod } from "@/model/service"

/** Names a group row lists before "+N". */
const NAMES = 2

/** One cause and the services it stops: a service's own row when it stops only one. */
export function ProblemGroupRow({ project, group }: { project: string; group: ProblemGroup }) {
  const t = useT()
  const write = useSentence()
  const [expanded, setExpanded] = useState(false)
  const [first] = group.services
  if (!first) return null
  if (group.services.length === 1) return <ProblemRow project={project} service={first} />
  const d = group.diagnosis
  const cause = d ? sharedCause(d.cause, t("attention.eachOne")) : undefined
  const action = d ? diagnosisAction(d) : undefined
  const pod = action?.pod ?? worstPod(first)?.name
  const names = expanded ? group.services : group.services.slice(0, NAMES)
  const hidden = group.services.length - names.length
  const button = buttonVariants({ variant: "outline", size: "sm" })
  return (
    <li className="flex flex-wrap items-center gap-x-4 gap-y-2.5 border-t border-rule px-4 py-3 first:border-t-0">
      <ServiceIcon service={first} />
      <div className="flex min-w-60 flex-1 flex-col gap-1">
        <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1">
          <span className="font-semibold">{t("attention.services", group.services.length)}</span>
          <ServiceStatus status={statusOf(first)} size="sm" />
          <span className="min-w-0 type-meta text-muted-foreground">
            {names.map((service, i) => (
              <Fragment key={serviceId(service)}>
                {i > 0 ? t("common.sep") : null}
                <Link
                  {...serviceLink(project, service)}
                  className="hover:text-foreground hover:underline"
                  translate="no"
                >
                  {service.name}
                </Link>
              </Fragment>
            ))}
            {hidden > 0 ? (
              <button
                type="button"
                onClick={() => setExpanded(true)}
                aria-label={t("attention.moreOf", hidden)}
                className="ml-1.5 rounded outline-none hover:text-foreground hover:underline"
              >
                {t("attention.more", hidden)}
              </button>
            ) : null}
          </span>
        </div>
        {cause || d ? (
          <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1">
            {cause ? <span className="type-meta text-foreground">{write(cause)}</span> : null}
            {d ? <FixerLine fixer={originOf(d).fixer} /> : null}
          </div>
        ) : null}
      </div>
      <div className="flex shrink-0 flex-wrap items-center gap-1.5">
        {action && action.tab !== "logs" ? (
          <Link
            {...serviceLink(project, first, { tab: action.tab, pod: action.pod })}
            className={button}
          >
            {t(action.label)}
          </Link>
        ) : (
          <Link
            {...serviceLink(project, first, { tab: "logs", pod })}
            className={button}
            aria-label={t("attention.logsOf", pod ?? first.name)}
          >
            <FileTextIcon aria-hidden="true" data-icon="inline-start" />
            {t("tab.logs")}
          </Link>
        )}
      </div>
    </li>
  )
}
