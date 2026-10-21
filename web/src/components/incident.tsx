import { Link } from "@tanstack/react-router"

import type { Schemas } from "@/api/client"
import { Ago } from "@/components/bits"
import { fixLead } from "@/components/origin"
import { StatusDot } from "@/components/status"
import { useSentence, useT } from "@/i18n"
import { serviceLink } from "@/lib/place"

export type Incident = Schemas["Incident"]

const key = (s: { project: string; kind: string; name: string }) =>
  `${s.project}/${s.kind}/${s.name}`

/** The services an incident takes down, as `project/Kind/name`. */
export const takenDown = (incidents: readonly Incident[]) =>
  new Set(incidents.flatMap((i) => i.services.map(key)))

export const isTakenDown = (
  down: ReadonlySet<string>,
  project: string,
  service: { kind: string; name: string },
) => down.has(key({ project, kind: service.kind, name: service.name }))

/** One cause in the cluster, once, with what it takes down under it. */
export function IncidentRow({
  incident,
  showProject = true,
}: {
  incident: Incident
  showProject?: boolean
}) {
  const t = useT()
  const write = useSentence()
  const where =
    incident.kind === "nodeNotReady" ? (
      <Link to="/cluster" hash="cluster-nodes" className="hover:text-foreground hover:underline">
        {t("cluster.nodes")}
      </Link>
    ) : (
      <Link to="/cluster/components" className="hover:text-foreground hover:underline">
        {t("settings.components")}
      </Link>
    )
  return (
    <li className="flex flex-wrap items-start gap-x-3 gap-y-1 border-t border-rule px-4 py-3 first:border-t-0">
      <StatusDot health="fail" className="mt-1.5" />
      <div className="flex min-w-60 flex-1 flex-col gap-1">
        <span className="font-medium">{write(incident.cause)}</span>
        <span className="type-meta text-muted-foreground">
          {fixLead(t, incident.fault.fixer)}
          {write(incident.fix)}
        </span>
        {incident.services.length > 0 ? (
          <span className="type-meta text-muted-foreground">
            {t("home.affects")}
            {t("common.colon")}
            {incident.services.map((s, i) => (
              <span key={s.id}>
                {i > 0 ? t("common.sep") : null}
                <Link
                  {...serviceLink(s.project, s)}
                  className="text-foreground hover:underline"
                  translate="no"
                >
                  {showProject ? `${s.project}/${s.name}` : s.name}
                </Link>
              </span>
            ))}
            {incident.domains.length > 0 ? (
              <>
                {t("common.sep")}
                {t("incident.domains", incident.domains.length)}
              </>
            ) : null}
          </span>
        ) : null}
      </div>
      <span className="flex items-center gap-3 type-meta text-muted-foreground">
        {incident.since ? <Ago iso={incident.since} /> : null}
        {where}
      </span>
    </li>
  )
}
