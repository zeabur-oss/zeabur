import type { Schemas } from "@/api/client"
import type { Health, Service } from "@/model/service"

export type Alert = Schemas.FalcoAlert

/** The names a workload's controller gives its pods; a gone pod still matches. */
const suffix: Record<Service["kind"], string> = {
  Deployment: "-[a-z0-9]{1,10}-[a-z0-9]{5}",
  StatefulSet: "-[0-9]+",
  DaemonSet: "-[a-z0-9]{5}",
  Job: "-[a-z0-9]{5}",
  CronJob: "-[0-9]+-[a-z0-9]{5}",
}

const escape = (name: string) => name.replace(/[.]/g, "[.]")

/** The alerts about the service's pods: its pods now, or a pod its controller would name. */
export function alertsFor(alerts: Alert[], service: Service): Alert[] {
  const pods = new Set(service.pods.map((p) => p.name))
  const named = new RegExp(`^${escape(service.name)}${suffix[service.kind]}$`)
  return alerts.filter((a) => !!a.pod && (pods.has(a.pod) || named.test(a.pod)))
}

/** The service whose pod an alert concerns, if any. */
export const serviceOf = (alert: Alert, services: Service[]) =>
  services.find((s) => alertsFor([alert], s).length > 0)

export interface Folded {
  alert: Alert
  /** How many times it fired; `alert` is the newest. */
  count: number
}

/** The same rule on the same container running the same command, once, newest first. */
export function folded(alerts: Alert[]): Folded[] {
  const seen = new Map<string, Folded>()
  for (const alert of alerts) {
    const key = [alert.rule, alert.pod, alert.container, alert.command].join("\u0000")
    const known = seen.get(key)
    if (known) known.count += 1
    else seen.set(key, { alert, count: 1 })
  }
  return [...seen.values()]
}

export const severityHealth: Record<Alert["severity"], Health> = {
  critical: "fail",
  high: "fail",
  medium: "warn",
  low: "idle",
  info: "idle",
}
