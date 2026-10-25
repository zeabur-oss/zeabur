// The plans a gap's fix names (`compute.resources`, `probes.readiness`), and
// the forms that end in a plan when a fix needs input or a template declines its case. Every
// template only proposes: the plan dialog shows the change and the person applies it or not.
import type { Schemas } from "@/api/client"
import type { PlanAsk } from "@/components/plan/planner"
import type { Translate } from "@/i18n"
import { databaseLink, serviceLink } from "@/lib/place"
import { endpointsOf } from "@/model/service"

type Gap = Schemas["Gap"]
type Project = Schemas["Project"]
type Service = Project["services"][number]

/** Builds the plan that fixes a gap, or `undefined` when this gap's case needs a person. */
export type FixTemplate = (gap: Gap, project: Project, t: Translate) => PlanAsk | undefined

/** Starting values for a container that sets none: small enough for a 2 GB node, a memory limit
 * at twice the request. The person sees them in the plan before anything is written. */
const STARTING = { cpu: "100m", memory: "128Mi", memoryLimit: "256Mi" } as const

/** The size a new disk starts at, as in the storage form. */
const DISK_SIZE = "1Gi"

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

function policyFix(gap: Gap, project: Project, t: Translate, side: "from" | "to") {
  const object = gap.fix.params?.object
  const from = project.services.find((s) => s.id === gap.fix.params?.from)
  const to = project.services.find((s) => s.id === gap.fix.params?.to)
  if (!object || !from || !to) return undefined
  return {
    title: t("reach.planAllowFrom", from.name, to.name),
    changes: [{ op: "create" as const, object: JSON.parse(object) as Record<string, never> }],
    service: workloadOf(side === "from" ? from : to),
  }
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
  // The NetworkPolicy module's ready-made policies: one that lets a service accept a connection,
  // and one that lets the sender's pods connect out. Each is created as a new object.
  "networkPolicy.allowFrom": (gap, project, t) => policyFix(gap, project, t, "to"),
  "networkPolicy.allowEgress": (gap, project, t) => policyFix(gap, project, t, "from"),
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
  // A disk at the directory the image keeps its data in. A temporary volume already mounted
  // there must be replaced by a person, who knows whether its data matters.
  "storage.addVolume": (gap, project, t) => {
    const found = target(gap, project)
    const mountPath = gap.fix.params?.mountPath
    if (!found || !mountPath?.startsWith("/")) return undefined
    if (found.container.mounts.some((m) => m.mountPath === mountPath)) return undefined
    return {
      title: t("fix.addVolume", found.container.name, mountPath),
      changes: [
        {
          op: "addVolume",
          workload: workloadOf(found.service),
          container: found.container.name,
          volume: { mountPath, size: DISK_SIZE },
        },
      ],
      service: workloadOf(found.service),
    }
  },
}

export type FixLink =
  | ReturnType<typeof serviceLink>
  | ReturnType<typeof databaseLink>
  | { to: "/cluster/backups" }

/** The form that asks for what a fix needs and ends in a plan. */
export function fixLink(
  gap: Gap,
  project: Project,
): { link: FixLink; label: "gap.setUpBackups" | "gap.openSettings" } | undefined {
  const params = gap.fix.params ?? {}
  const service = project.services.find((s) => s.id === (gap.service ?? params.service))
  const settings = (tab: "compute" | "health-checks" | "storage" | "networking") =>
    service
      ? {
          link: serviceLink(project.namespace, service, { tab }),
          label: "gap.openSettings" as const,
        }
      : undefined
  switch (gap.fix.id) {
    case "backups.scheduledBackup":
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
    case "backups.k8upSchedule": {
      // A StatefulSet's claim is `<template>-<statefulset>-<ordinal>`.
      const owner = project.services.find((s) =>
        s.containers.some((c) =>
          c.mounts.some(
            (m) =>
              m.claimName === params.claim ||
              (s.kind === "StatefulSet" && params.claim?.startsWith(`${m.claimName}-${s.name}-`)),
          ),
        ),
      )
      return owner
        ? {
            link: serviceLink(project.namespace, owner, { tab: "storage" }),
            label: "gap.setUpBackups",
          }
        : undefined
    }
    case "backups.veleroSchedule":
      return { link: { to: "/cluster/backups" }, label: "gap.setUpBackups" }
    case "compute.resources":
      return settings("compute")
    case "probes.readiness":
      return settings("health-checks")
    case "storage.addVolume":
      return settings("storage")
    case "domains.httpRoute":
      return settings("networking")
    default:
      return undefined
  }
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
