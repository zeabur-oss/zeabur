// A project as a card on the Projects page: its services' health, pods, warnings and first domain.
import { Link } from "@tanstack/react-router"
import CaretRightIcon from "~icons/ph/caret-right"
import GlobeIcon from "~icons/ph/globe"
import WarningIcon from "~icons/ph/warning"

import type { Schemas } from "@/api/client"
import { ProjectIcon } from "@/components/project-icon"
import { ServiceIcon } from "@/components/service-icon"
import { StatusDot } from "@/components/status"
import { Skeleton } from "@zeabur/design-system/components/skeleton"
import { Tooltip } from "@zeabur/design-system/components/tooltip"
import { useT } from "@/i18n"
import { cn } from "@/lib/utils"
import { isWarning, serviceId, statusOf, type Event, type ProjectSummary } from "@/model/service"

export function ProjectCard({
  name,
  details,
  project,
  summary,
  events,
}: {
  name: string
  /** The display name and icon the namespace's annotations give it. */
  details?: Pick<Schemas["ProjectSummary"], "displayName" | "icon">
  project?: Schemas["Project"]
  summary?: ProjectSummary
  events: Event[]
}) {
  const t = useT()
  const warnings = events.filter(isWarning).length
  return (
    <Link
      to="/projects/$project"
      params={{ project: name }}
      className="flex w-full flex-col gap-4 rounded-lg border bg-card shadow-xs p-4 outline-none transition-colors hover:border-foreground/20"
    >
      <div className="flex items-start gap-3">
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          <span className="flex items-center gap-2">
            <ProjectIcon icon={details?.icon} />
            <span className="truncate font-semibold" translate="no">
              {details?.displayName ?? name}
            </span>
          </span>
          {summary ? (
            summary.services > 0 ? (
              <span className="text-xs text-muted-foreground">
                {t("project.healthyOf", summary.healthy, summary.services)}
              </span>
            ) : (
              <span className="text-xs text-muted-foreground">{t("home.noServices")}</span>
            )
          ) : (
            <Skeleton className="h-4 w-32" />
          )}
        </div>
        <CaretRightIcon className="size-4 text-muted-foreground" aria-hidden="true" />
      </div>

      {project && project.services.length > 0 ? (
        <div className="flex flex-wrap gap-1.5">
          {project.services.map((s) => (
            <Tooltip key={serviceId(s)} content={`${s.name}${t("common.sep")}${s.kind}`}>
              <span className="relative">
                <ServiceIcon service={s} size="sm" />
                <StatusDot
                  health={statusOf(s).health}
                  className="absolute -right-0.5 -bottom-0.5 size-2"
                />
              </span>
            </Tooltip>
          ))}
        </div>
      ) : null}

      <div className="mt-auto grid grid-cols-3 gap-2 border-t pt-3 text-xs">
        <span className="flex flex-col gap-0.5">
          <span className="text-muted-foreground">{t("metric.warnings")}</span>
          <span
            className={cn(
              "flex items-center gap-1 font-medium tabular-nums",
              warnings > 0 && "text-fail",
            )}
          >
            {warnings > 0 ? <WarningIcon className="size-3.5" aria-hidden="true" /> : null}
            {warnings}
          </span>
        </span>
        <span className="flex flex-col gap-0.5">
          <span className="text-muted-foreground">{t("pod.pods")}</span>
          <span className="font-medium tabular-nums">
            {summary ? `${summary.podsReady}/${summary.pods}` : "—"}
          </span>
        </span>
        <span className="flex min-w-0 flex-col gap-0.5">
          <span className="text-muted-foreground">{t("common.domains")}</span>
          <span className="flex min-w-0 items-center gap-1 font-medium">
            {summary && summary.domains.length > 0 ? (
              <>
                <GlobeIcon className="size-3.5 shrink-0 text-muted-foreground" aria-hidden="true" />
                <span className="truncate font-mono type-micro" translate="no">
                  {summary.domains[0]?.host}
                </span>
                {summary.domains.length > 1 ? (
                  <span className="text-muted-foreground">+{summary.domains.length - 1}</span>
                ) : null}
              </>
            ) : (
              "—"
            )}
          </span>
        </span>
      </div>
    </Link>
  )
}
