// The plans a gap's fix names (`compute.resources`, `probes.readiness`), and
// the forms that end in a plan when a fix needs input or a template declines its case. Every
// template only proposes: the plan dialog shows the change and the person applies it or not.
// The server chooses the values and the workload; a template only writes them as a change.
import type { Schemas } from "@/api/client"
import type { PlanAsk } from "@/components/plan/planner"
import type { Translate } from "@/i18n"
import { databaseLink, serviceLink } from "@/lib/place"

type Gap = Schemas.Gap
type Project = Schemas.Project
type WorkloadRef = Schemas.WorkloadRef
type FixValues = Schemas.FixValues

/** Builds the plan that fixes a gap, or `undefined` when this gap's case needs a person. */
export type FixTemplate = (gap: Gap, project: Project, t: Translate) => PlanAsk | undefined

/** The fix's workload and values when the values are of the kind `set`. */
function valuesOf<S extends FixValues["set"]>(gap: Gap, set: S) {
  const { workload, values } = gap.fix
  return workload && values?.set === set
    ? { workload, values: values as Extract<FixValues, { set: S }> }
    : undefined
}

const workloadOf = (service: { kind: WorkloadRef["kind"]; name: string }) => ({
  kind: service.kind,
  name: service.name,
})

function policyFix(gap: Gap, project: Project, t: Translate, side: "from" | "to") {
  const object = gap.fix.params?.object
  const from = project.services.find((s) => s.id === gap.fix.params?.from)
  const to = project.services.find((s) => s.id === gap.fix.params?.to)
  if (!object || !from || !to) return undefined
  return {
    title: t("reach.planAllowFrom", from.name, to.name),
    changes: [{ op: "create" as const, object: JSON.parse(object) as Record<string, unknown> }],
    service: workloadOf(side === "from" ? from : to),
  }
}

export const fixTemplates: Record<string, FixTemplate> = {
  "compute.resources": (gap, _project, t) => {
    const found = valuesOf(gap, "resources")
    if (!found) return undefined
    const { container, requests, limits } = found.values
    return {
      title: t("fix.resources", container),
      changes: [{ op: "resources", workload: found.workload, container, requests, limits }],
    }
  },
  // The NetworkPolicy module's ready-made policies: one that lets a service accept a connection,
  // and one that lets the sender's pods connect out. Each is created as a new object.
  "networkPolicy.allowFrom": (gap, project, t) => policyFix(gap, project, t, "to"),
  "networkPolicy.allowEgress": (gap, project, t) => policyFix(gap, project, t, "from"),
  // The ready-made policies that isolate a project's unprotected services, and let the ingress
  // controller reach those it routes to: one plan, each policy created as a new object.
  "networkPolicy.isolateProject": (gap, project, t) => {
    const objects = gap.fix.params?.objects
    if (!objects) return undefined
    const changes = (JSON.parse(objects) as Record<string, unknown>[]).map((object) => ({
      op: "create" as const,
      object,
    }))
    if (changes.length === 0) return undefined
    return { title: t("fix.isolateProject", project.namespace), changes }
  },
  "probes.readiness": (gap, _project, t) => {
    const found = valuesOf(gap, "tcpReadiness")
    if (!found) return undefined
    const { container, port, periodSeconds } = found.values
    return {
      title: t("fix.readiness", container),
      changes: [
        {
          op: "probe",
          workload: found.workload,
          container,
          probe: "readiness",
          set: { handler: "TCPSocket", port, periodSeconds },
        },
      ],
    }
  },
  "storage.addVolume": (gap, _project, t) => {
    const found = valuesOf(gap, "volume")
    if (!found) return undefined
    const { container, mountPath, size } = found.values
    return {
      title: t("fix.addVolume", container, mountPath),
      changes: [
        { op: "addVolume", workload: found.workload, container, volume: { mountPath, size } },
      ],
      service: found.workload,
    }
  },
}

export type FixLink =
  | ReturnType<typeof serviceLink>
  | ReturnType<typeof databaseLink>
  | { to: "/cluster/backups" }

type FixLabel = "gap.setUpBackups" | "gap.openSettings"

/** The service page tab where a person sets what a fix needs. */
const settingsTab: Partial<Record<string, "compute" | "health-checks" | "storage" | "networking">> =
  {
    "compute.resources": "compute",
    "probes.readiness": "health-checks",
    "storage.addVolume": "storage",
    "domains.httpRoute": "networking",
  }

/** The form that asks for what a fix needs and ends in a plan. */
export function fixLink(
  gap: Gap,
  project: Project,
): { link: FixLink; label: FixLabel } | undefined {
  const { id, params = {}, workload } = gap.fix
  const service = project.services.find((s) => s.id === gap.service)
  const owner = workload ?? (service && workloadOf(service))
  const page = (tab: "compute" | "health-checks" | "storage" | "networking", label: FixLabel) =>
    owner ? { link: serviceLink(project.namespace, owner, { tab }), label } : undefined
  if (id === "backups.scheduledBackup")
    return params.database
      ? {
          link: databaseLink(
            project.namespace,
            { engine: "postgresql", name: params.database },
            "backups",
          ),
          label: "gap.setUpBackups",
        }
      : undefined
  if (id === "backups.k8upSchedule") return page("storage", "gap.setUpBackups")
  if (id === "backups.veleroSchedule")
    return { link: { to: "/cluster/backups" }, label: "gap.setUpBackups" }
  const tab = settingsTab[id]
  return tab ? page(tab, "gap.openSettings") : undefined
}

/** A gap's fix: the plan a template writes, or else the form that ends in one. */
export function fixOf(
  gap: Gap,
  project: Project | undefined,
  t: Translate,
): { ask?: PlanAsk; form?: ReturnType<typeof fixLink> } {
  if (!project) return {}
  const ask = fixTemplates[gap.fix.id]?.(gap, project, t)
  return ask ? { ask } : { form: fixLink(gap, project) }
}
