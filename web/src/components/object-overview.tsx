// An object's status and events, the way `kubectl describe` lists them.
import { useQuery } from "@tanstack/react-query"
import type { ReactNode } from "react"
import CaretRightIcon from "~icons/ph/caret-right"

import { objectEventsQuery, type ObjectAddress } from "@/api/queries"
import { Ago } from "@/components/bits"
import { EventList } from "@/components/events"
import { LoadingRows } from "@/components/page"
import { Problem } from "@/components/problem"
import { StatusDot } from "@/components/status"
import { YamlBlock } from "@/components/yaml"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur/design-system/components/collapsible"
import { useT } from "@/i18n"
import { conditionHealth, statusParts, toYaml, type Condition } from "@/model/object-status"

export function ObjectOverview({
  project,
  address,
  kind,
  status,
}: {
  project: string
  address: ObjectAddress
  kind: string
  status?: Record<string, unknown> | null
}) {
  const t = useT()
  const parts = statusParts(status)
  const reported = parts.conditions.length > 0 || parts.fields.length > 0 || parts.nested
  return (
    <div className="flex flex-col gap-6">
      {parts.conditions.length > 0 ? (
        <Section id="object-conditions" title={t("object.conditions")}>
          <ol className="flex flex-col" aria-labelledby="object-conditions">
            {parts.conditions.map((condition) => (
              <ConditionRow key={condition.type} condition={condition} />
            ))}
          </ol>
        </Section>
      ) : null}
      {!reported ? (
        <Section id="object-status" title={t("object.status")}>
          <p className="py-3 text-sm text-muted-foreground">{t("object.noStatus")}</p>
        </Section>
      ) : parts.fields.length > 0 || parts.nested ? (
        <Section id="object-status" title={t("object.status")}>
          <div className="flex flex-col gap-3 border-t border-rule pt-3">
            {parts.fields.length > 0 ? (
              <dl className="grid grid-cols-pairs gap-x-6 gap-y-1.5 text-sm">
                {parts.fields.map(([key, value]) => (
                  <div key={key} className="contents">
                    <dt className="font-mono text-muted-foreground" translate="no">
                      {key}
                    </dt>
                    <dd className="min-w-0 font-mono break-all" translate="no">
                      {value}
                    </dd>
                  </div>
                ))}
              </dl>
            ) : null}
            {parts.nested ? (
              <Collapsible>
                <CollapsibleTrigger variant="disclosure">
                  <CaretRightIcon aria-hidden="true" />
                  {t("object.moreStatus", Object.keys(parts.nested).join(", "))}
                </CollapsibleTrigger>
                <CollapsibleContent>
                  <YamlBlock
                    yaml={toYaml(parts.nested)}
                    label={t("object.statusLabel", address.name)}
                    className="mt-2"
                  />
                </CollapsibleContent>
              </Collapsible>
            ) : null}
          </div>
        </Section>
      ) : null}
      {address.resource === "events" ? null : (
        <Section id="object-events" title={t("project.events")}>
          <ObjectEvents project={project} kind={kind} name={address.name} />
        </Section>
      )}
    </div>
  )
}

function Section({ id, title, children }: { id: string; title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-2" aria-labelledby={id}>
      <h3 id={id} className="type-meta font-medium text-muted-foreground">
        {title}
      </h3>
      {children}
    </section>
  )
}

function ConditionRow({ condition }: { condition: Condition }) {
  return (
    <li className="grid grid-cols-event gap-x-3 border-t border-rule py-3">
      <StatusDot health={conditionHealth(condition)} className="mt-1.5" />
      <div className="flex min-w-0 flex-col gap-0.5">
        <div className="flex min-w-0 flex-wrap items-baseline gap-x-2 text-sm" translate="no">
          <span className="font-semibold">{condition.type}</span>
          <span className="type-meta text-muted-foreground">{condition.status}</span>
          {condition.reason ? (
            <span className="min-w-0 truncate type-mono text-muted-foreground">
              {condition.reason}
            </span>
          ) : null}
        </div>
        {condition.message ? (
          <p className="text-sm break-words text-muted-foreground">{condition.message}</p>
        ) : null}
      </div>
      <Ago iso={condition.lastTransitionTime} className="type-meta text-muted-foreground" />
    </li>
  )
}

function ObjectEvents({ project, kind, name }: { project: string; kind: string; name: string }) {
  const t = useT()
  const events = useQuery(objectEventsQuery(project, kind, name))
  if (events.isPending) return <LoadingRows label={t("common.loading")} rows={3} />
  if (events.isError)
    return <Problem error={events.error} what={t("refused.see", t("project.events"))} />
  return <EventList events={events.data.items} oneObject />
}
