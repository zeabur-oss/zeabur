// The plan templates a gap's fix names (`compute.resources`, `probes.readiness`,
// `domains.httpRoute`) that this build can write through a plan. A gap whose template is missing
// here shows the server's YAML snippet to copy instead, so a template can be added without the
// rule or the server changing. Every template only proposes: the plan dialog shows the change
// and the person applies it or not.
import type { Schemas } from "@/api/client"
import type { PlanAsk } from "@/components/plan/planner"
import type { Translate } from "@/i18n"
import { endpointsOf } from "@/model/service"

type Gap = Schemas["GapView"]
type Project = Schemas["Project"]
type Service = Project["services"][number]

/** Builds the plan that fixes a gap, or `undefined` when this gap's case needs a person. */
export type FixTemplate = (gap: Gap, project: Project, t: Translate) => PlanAsk | undefined

/** Starting values for a container that sets none: small enough for a 2 GB node, a memory limit
 * at twice the request. The person sees them in the plan before anything is written. */
const STARTING = { cpu: "100m", memory: "128Mi", memoryLimit: "256Mi" } as const

function target(gap: Gap, project: Project) {
  const service = project.services.find((s) => s.id === gap.fix.params?.service)
  const name = gap.fix.params?.container
  const container = service?.containers.find((c) => c.name === name)
  return service && container ? { service, container } : undefined
}

const workloadOf = (service: Service) => ({ kind: service.kind, name: service.name })

/** The container's first numeric port, from the Service in front of it. */
function portOf(project: Project, service: Service): number | undefined {
  for (const endpoint of endpointsOf(project, service))
    for (const port of endpoint.ports) {
      const target = port.targetPort ?? String(port.port)
      if (/^\d+$/.test(target)) return Number(target)
    }
  return undefined
}

export const fixTemplates: Record<string, FixTemplate> = {
  // Fill in only what is missing; what the container already sets is kept as it is.
  "compute.resources": (gap, project, t) => {
    const found = target(gap, project)
    if (!found) return undefined
    const { requests, limits } = found.container.resources ?? {}
    const pick = (entries: [string, string | null | undefined][]) =>
      Object.fromEntries(entries.filter((e): e is [string, string] => !!e[1]))
    return {
      title: t("fix.resources", found.container.name),
      changes: [
        {
          op: "resources",
          workload: workloadOf(found.service),
          container: found.container.name,
          requests: pick([
            ["cpu", requests?.cpu ?? STARTING.cpu],
            ["memory", requests?.memory ?? STARTING.memory],
          ]),
          limits: pick([
            ["cpu", limits?.cpu],
            ["memory", limits?.memory ?? STARTING.memoryLimit],
          ]),
        },
      ],
    }
  },
  // The NetworkPolicy module's ready-made allow-from-service policy, which the server put in the
  // gap: created as a new object.
  "networkPolicy.allowFrom": (gap, project, t) => {
    const object = gap.fix.params?.object
    const from = project.services.find((s) => s.id === gap.fix.params?.from)
    const to = project.services.find((s) => s.id === gap.fix.params?.to)
    if (!object || !from || !to) return undefined
    return {
      title: t("reach.planAllowFrom", from.name, to.name),
      changes: [{ op: "create", object: JSON.parse(object) as Record<string, never> }],
      service: workloadOf(to),
    }
  },
  // The ready-made policies that isolate a project's unprotected services, and let the ingress
  // controller reach those it routes to: one plan, each policy created as a new object.
  "networkPolicy.isolateProject": (gap, project, t) => {
    const objects = gap.fix.params?.objects
    if (!objects) return undefined
    const changes = (JSON.parse(objects) as Record<string, never>[]).map((object) => ({
      op: "create" as const,
      object,
    }))
    if (changes.length === 0) return undefined
    return { title: t("fix.isolateProject", project.namespace), changes }
  },
  // A TCP check on the port the Service sends traffic to: it holds traffic back until the
  // process listens, without guessing an HTTP path. No numeric port: a person must choose.
  "probes.readiness": (gap, project, t) => {
    const found = target(gap, project)
    const port = found && portOf(project, found.service)
    if (!found || port === undefined) return undefined
    return {
      title: t("fix.readiness", found.container.name),
      changes: [
        {
          op: "probe",
          workload: workloadOf(found.service),
          container: found.container.name,
          probe: "readiness",
          set: { handler: "TCPSocket", port, periodSeconds: 10 },
        },
      ],
    }
  },
}
