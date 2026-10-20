import { Link } from "@tanstack/react-router"

import { Tooltip } from "@zeabur/design-system/components/tooltip"
import { useT } from "@/i18n"
import { podLogsLink } from "@/lib/place"
import { cn } from "@/lib/utils"
import { podHealth, podReason, type Pod, type Service } from "@/model/service"

const cellColour = {
  fail: "bg-fail",
  warn: "bg-hatch bg-warn/30 text-warn",
  ok: "bg-ok/85",
  idle: "hairline-ring ring-idle ring-inset",
  progress: "bg-progress",
} as const

const cellBox = "size-2 rounded-xs"

function usePodLabel() {
  const t = useT()
  return (pod: Pod) => {
    const reason = podReason(pod)
    return [
      pod.name,
      reason ?? (pod.ready ? t("pod.ready") : t("pod.notReady")),
      t("pod.restarts", pod.restarts),
    ].join(t("common.sep"))
  }
}

/** The controller's count where there is one: during a rollout old and new pods overlap. */
export function replicaCounts(service: Service): { ready: number; desired: number } {
  return (
    service.replicas ?? {
      ready: service.pods.filter((pod) => pod.ready).length,
      desired: service.pods.length,
    }
  )
}

export function PodCells({
  service,
  project,
  max = 10,
}: {
  service: Service
  /** Given, each square links to the pod's logs. */
  project?: string
  max?: number
}) {
  const t = useT()
  const label = usePodLabel()
  const { ready, desired } = replicaCounts(service)
  if (service.pods.length === 0 && desired === 0)
    return <span className="text-xs text-muted-foreground">—</span>
  return (
    <span className="flex items-center gap-2">
      <span className={cn("tabular-nums", ready < desired && "text-warn-strong")}>
        {ready}/{desired}
      </span>
      <ul className="flex items-center gap-0.5" aria-label={t("pod.pods")}>
        {service.pods.slice(0, max).map((pod) => {
          const health = podHealth(pod)
          const className = cn(
            "inline-flex items-center justify-center outline-none",
            cellBox,
            cellColour[health],
          )
          return (
            <li key={pod.name} className="flex">
              {project ? (
                <Tooltip content={label(pod)}>
                  <Link
                    {...podLogsLink(project, service, pod.name)}
                    aria-label={t("pod.logsOf", pod.name)}
                    className={className}
                    onClick={(event) => event.stopPropagation()}
                  />
                </Tooltip>
              ) : (
                <span className={className} title={label(pod)} />
              )}
            </li>
          )
        })}
        {service.pods.length > max ? (
          <li className="pl-0.5 type-micro text-muted-foreground">+{service.pods.length - max}</li>
        ) : null}
      </ul>
    </span>
  )
}
