import { Link } from "@tanstack/react-router"
import type { ReactNode } from "react"
import CaretRightIcon from "~icons/ph/caret-right"

import type { Schemas } from "@/api/client"
import { Ago } from "@/components/bits"
import { fixLead } from "@/components/origin"
import { StatusDot } from "@/components/status"
import { useSentence, useT } from "@/i18n"
import { serviceLink } from "@/lib/place"
import { groupIncidents, serviceKey as key, type IncidentGroup } from "@/model/incidents"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur/design-system/components/collapsible"

export type Incident = Schemas["Incident"]
type Affected = Schemas["AffectedService"]

/** The services an incident takes down, as `project/Kind/name`. */
export const takenDown = (incidents: readonly Incident[]) =>
  new Set(incidents.flatMap((i) => i.services.map(key)))

export const isTakenDown = (
  down: ReadonlySet<string>,
  project: string,
  service: { kind: string; name: string },
) => down.has(key({ project, kind: service.kind, name: service.name }))

/** Up to this many services are named in the row; more go behind a disclosure. */
const NAMED = 3

/** One cause in the cluster, once, with what it takes down under it. */
export function IncidentRow({
  incident,
  showProject = true,
}: {
  incident: Incident
  showProject?: boolean
}) {
  const [group] = groupIncidents([incident])
  return group ? <IncidentGroupRow group={group} showProject={showProject} /> : null
}

export function IncidentGroupRow({
  group,
  showProject = true,
}: {
  group: IncidentGroup
  showProject?: boolean
}) {
  const t = useT()
  const write = useSentence()
  const first = group.incidents[0]
  if (!first) return null
  const many = group.incidents.length > 1
  const unknown = first.cause.params?.status === "Unknown"
  const where =
    first.kind === "nodeNotReady" ? (
      <Link to="/cluster" hash="cluster-nodes" className="hover:text-foreground hover:underline">
        {t("cluster.nodes")}
      </Link>
    ) : (
      <Link to="/cluster/components" className="hover:text-foreground hover:underline">
        {t("settings.components")}
      </Link>
    )
  const since = group.incidents
    .map((i) => i.since)
    .filter((s): s is string => !!s)
    .toSorted()[0]
  const serviceLinkOf = (s: Affected) => (
    <Link {...serviceLink(s.project, s)} className="text-foreground hover:underline" translate="no">
      {showProject ? `${s.project}/${s.name}` : s.name}
    </Link>
  )
  return (
    <li className="flex flex-wrap items-start gap-x-3 gap-y-1 border-t border-rule px-4 py-3 first:border-t-0">
      <StatusDot health="fail" className="mt-1.5" />
      <div className="flex min-w-0 flex-1 basis-60 flex-col gap-1">
        <span className="font-medium">
          {many
            ? unknown
              ? t("incident.nodesNotReporting", group.incidents.length)
              : t("incident.nodesNotReady", group.incidents.length)
            : write(first.cause)}
        </span>
        <span className="type-meta text-muted-foreground">
          {fixLead(t, first.fault.fixer)}
          {many ? t("incident.nodesNotReady.fix") : write(first.fix)}
        </span>
        {many ? (
          <Members
            label={t("cluster.showMembers", group.incidents.length)}
            items={group.incidents.map((i) => ({
              key: i.id,
              node: <span translate="no">{i.cause.params?.node ?? i.id}</span>,
              note: i.since ? <Ago iso={i.since} /> : null,
            }))}
          />
        ) : null}
        {group.services.length > 0 && group.services.length <= NAMED ? (
          <span className="type-meta text-muted-foreground">
            {t("home.affects")}
            {t("common.colon")}
            {group.services.map((s, i) => (
              <span key={s.id}>
                {i > 0 ? t("common.sep") : null}
                {serviceLinkOf(s)}
              </span>
            ))}
            {group.domains > 0 ? (
              <>
                {t("common.sep")}
                {t("incident.domains", group.domains)}
              </>
            ) : null}
          </span>
        ) : null}
        {group.services.length > NAMED ? (
          <Members
            label={[
              showProject
                ? t("incident.servicesIn", group.services.length, group.projects)
                : t("incident.services", group.services.length),
              group.domains > 0 ? t("incident.domains", group.domains) : "",
            ]
              .filter(Boolean)
              .join(t("common.sep"))}
            items={group.services.map((s) => ({ key: s.id, node: serviceLinkOf(s) }))}
          />
        ) : null}
      </div>
      <span className="flex items-center gap-3 type-meta text-muted-foreground">
        {since ? <Ago iso={since} /> : null}
        {where}
      </span>
    </li>
  )
}

function Members({
  label,
  items,
}: {
  label: string
  items: { key: string; node: ReactNode; note?: ReactNode }[]
}) {
  return (
    <Collapsible>
      <CollapsibleTrigger variant="disclosure">
        <CaretRightIcon aria-hidden="true" />
        {label}
      </CollapsibleTrigger>
      <CollapsibleContent>
        <ul className="mt-1 flex max-h-60 flex-col overflow-y-auto type-meta">
          {items.map((item) => (
            <li key={item.key} className="flex min-w-0 items-baseline gap-2 py-0.5">
              <span className="min-w-0 truncate">{item.node}</span>
              {item.note ? (
                <span className="shrink-0 text-muted-foreground">{item.note}</span>
              ) : null}
            </li>
          ))}
        </ul>
      </CollapsibleContent>
    </Collapsible>
  )
}
