// One cell per pod: ready (green), starting (ochre, hatched), failing (red), finished (a ring).
// Inside a table a cell links to that pod's logs, so the entry is as fine-grained as where it leads.
import { Link } from "@tanstack/react-router"

import { Tooltip } from "@/components/ui/tooltip"
import { useT } from "@/i18n"
import { podLogsLink } from "@/lib/place"
import { cn } from "@/lib/utils"
import { podHealth, podReason, type Pod, type Service } from "@/model/service"

export const cellColour = {
  fail: "bg-fail",
  warn: "bg-hatch bg-warn/30 text-warn",
  ok: "bg-ok/85",
  idle: "hairline-ring ring-idle ring-inset",
  progress: "bg-progress",
} as const

/** Cell sizes: `sm` in table rows, `md` in the pod strip. */
export const cellBox = { sm: "size-2.5 rounded-xs", md: "size-3 rounded-[3px]" } as const

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

export function PodCells({
  service,
  project,
  size = "sm",
  max = 24,
}: {
  service: Service
  /** When given, each cell links to the pod's logs. */
  project?: string
  size?: "sm" | "md"
  max?: number
}) {
  const t = useT()
  const label = usePodLabel()
  if (service.pods.length === 0) return <span className="text-xs text-muted-foreground">—</span>
  const box = cellBox[size]
  return (
    <ul className="flex flex-wrap items-center gap-0.75" aria-label={t("pod.pods")}>
      {service.pods.slice(0, max).map((pod) => {
        const health = podHealth(pod)
        const className = cn(
          "inline-flex items-center justify-center outline-none focus-visible:ring-2 focus-visible:ring-ring",
          box,
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
        <li className="type-micro text-muted-foreground">+{service.pods.length - max}</li>
      ) : null}
    </ul>
  )
}
